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

#[allow(clippy::disallowed_methods)]
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
