//! Re-register directory names after replacement; an inotify watch follows the
//! old inode and the backend may discard it without an explicit unwatch.
use super::*;
use std::collections::BTreeSet;

pub(super) fn invalidation(event: &notify::Event, selected: &Path, target: Option<&Path>) -> (bool, bool) {
    if matches!(event.kind, notify::EventKind::Access(_)) {
        return (false, false);
    }
    let mut relevant = event.need_rescan();
    let mut reset = relevant;
    // Inspect every path: a rename can name both the leaf and its lost parent.
    for path in &event.paths {
        for source in [Some(selected), target].into_iter().flatten() {
            if source.starts_with(path) {
                relevant = true;
                reset |= source != path;
            }
        }
    }
    (relevant, reset)
}

pub(super) fn rearm(
    watcher: &mut impl Watcher,
    selected: &Path,
    target: &Mutex<Option<PathBuf>>,
    registered: &mut BTreeSet<PathBuf>,
    reset: bool,
) -> Result<(), SourceError> {
    let selected = selected.to_path_buf();
    let previous = target.lock().map_err(|_| SourceError::WatchUnavailable)?.clone();
    let (resolved, required, anchors) = io::bounded(move || {
        if io::path_needs_download(&selected).unwrap_or(false) {
            return Err(SourceError::Unavailable);
        }
        let observed = canonical_path_sync(&selected).ok().or_else(|| {
            let linked = selected.parent()?.join(std::fs::read_link(&selected).ok()?);
            Some(canonical_path_sync(linked.parent()?).ok()?.join(linked.file_name()?))
        });
        let resolved = observed
            .map(|resolved| (resolved != selected).then_some(resolved))
            .unwrap_or(previous);
        let mut required = BTreeSet::from([selected.parent().ok_or(SourceError::WatchUnavailable)?.to_path_buf()]);
        if let Some(parent) = resolved.as_deref().and_then(Path::parent) {
            required.insert(parent.to_path_buf());
        }
        // Plan metadata queries behind the same deadline as data reads. If
        // several levels disappeared, move the anchor upwards in this event.
        let anchors: BTreeSet<_> = required
            .iter()
            .filter_map(|parent| {
                parent
                    .ancestors()
                    .skip(1)
                    .find(|ancestor| ancestor.is_dir())
                    .map(Path::to_path_buf)
            })
            .collect();
        Ok((resolved, required, anchors))
    })
    .map_err(|_| SourceError::WatchUnavailable)?;
    *target.lock().map_err(|_| SourceError::WatchUnavailable)? = resolved;
    let mut desired = required.clone();
    desired.extend(anchors.iter().cloned());
    let mut healthy = true;
    for directory in anchors.iter().chain(required.iter()) {
        if !reset && registered.contains(directory) {
            continue;
        }
        // Reissue even for the same spelling: the kernel registration may
        // have died with a deleted directory. Remove first because FSEvents
        // otherwise accumulates duplicate paths. The following stable read
        // closes this registration gap before publishing any candidate.
        if registered.contains(directory) {
            match watcher.unwatch(directory) {
                Ok(()) => {
                    registered.remove(directory);
                }
                Err(error)
                    if matches!(
                        error.kind,
                        notify::ErrorKind::WatchNotFound | notify::ErrorKind::PathNotFound
                    ) =>
                {
                    registered.remove(directory);
                }
                Err(_) => {
                    healthy = false;
                    continue;
                }
            }
        }
        if watcher.watch(directory, RecursiveMode::NonRecursive).is_ok() {
            registered.insert(directory.clone());
        } else {
            healthy = false;
        }
    }
    let obsolete: Vec<_> = registered.difference(&desired).cloned().collect();
    for old in obsolete {
        match watcher.unwatch(&old) {
            Ok(()) => {
                registered.remove(&old);
            }
            Err(error)
                if matches!(
                    error.kind,
                    notify::ErrorKind::WatchNotFound | notify::ErrorKind::PathNotFound
                ) =>
            {
                registered.remove(&old);
            }
            Err(_) => healthy = false,
        }
    }
    if healthy {
        Ok(())
    } else {
        Err(SourceError::WatchUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_multi_path_event_cannot_hide_a_lost_parent_behind_its_leaf() {
        let parent = Path::new("synthetic/selected");
        let selected = parent.join("synthetic.bib");
        let event = notify::Event::new(notify::EventKind::Remove(notify::event::RemoveKind::Any))
            .add_path(selected.clone())
            .add_path(parent.to_path_buf());
        assert_eq!(invalidation(&event, &selected, None), (true, true));
        let leaf =
            notify::Event::new(notify::EventKind::Modify(notify::event::ModifyKind::Any)).add_path(selected.clone());
        assert_eq!(invalidation(&leaf, &selected, None), (true, false));
        let unrelated = notify::Event::new(notify::EventKind::Any).add_path(PathBuf::from("synthetic/another.bib"));
        assert_eq!(invalidation(&unrelated, &selected, None), (false, false));
    }

    #[derive(Default)]
    struct FakeWatcher {
        paths: BTreeSet<PathBuf>,
        refuse: Option<PathBuf>,
    }
    impl Watcher for FakeWatcher {
        fn new<F: notify::EventHandler>(_: F, _: notify::Config) -> notify::Result<Self> {
            Ok(Self::default())
        }
        fn watch(&mut self, path: &Path, _: RecursiveMode) -> notify::Result<()> {
            if self.refuse.as_deref() == Some(path) || !path.is_dir() {
                return Err(notify::Error::new(notify::ErrorKind::PathNotFound));
            }
            self.paths.insert(path.to_path_buf());
            Ok(())
        }
        fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
            if self.paths.remove(path) {
                Ok(())
            } else {
                Err(notify::Error::watch_not_found())
            }
        }
        fn kind() -> notify::WatcherKind {
            notify::WatcherKind::NullWatcher
        }
    }

    #[test]
    fn replacement_reinstalls_the_parent_and_refusal_keeps_the_warning() {
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let root = root.path().canonicalize().unwrap_or_else(|e| panic!("{e}"));
        let parent = root.join("selected");
        std::fs::create_dir(&parent).unwrap_or_else(|e| panic!("{e}"));
        let selected = parent.join("synthetic.bib");
        std::fs::write(&selected, "@misc{before}").unwrap_or_else(|e| panic!("{e}"));
        let target = Mutex::new(None);
        let mut registered = BTreeSet::new();
        let mut watcher = FakeWatcher::default();
        assert_eq!(rearm(&mut watcher, &selected, &target, &mut registered, true), Ok(()));
        assert!(
            watcher.paths.contains(&root),
            "the surviving ancestor must see recreation"
        );
        std::fs::remove_dir_all(&parent).unwrap_or_else(|e| panic!("{e}"));
        watcher.paths.remove(&parent); // Linux drops this without telling the owner to unwatch.
        assert_eq!(
            rearm(&mut watcher, &selected, &target, &mut registered, true),
            Err(SourceError::WatchUnavailable)
        );
        std::fs::create_dir(&parent).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(&selected, "@misc{restored}").unwrap_or_else(|e| panic!("{e}"));
        watcher.refuse = Some(parent.clone());
        assert_eq!(
            rearm(&mut watcher, &selected, &target, &mut registered, true),
            Err(SourceError::WatchUnavailable)
        );
        watcher.refuse = None;
        assert_eq!(rearm(&mut watcher, &selected, &target, &mut registered, true), Ok(()));
        assert!(watcher.paths.contains(&parent), "future saves must be watched too");
    }
}
