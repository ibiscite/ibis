//! Bounded, read-only file loading and platform notifications. A failed reload
//! never produces an empty candidate: callers keep their last good snapshot.
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use thiserror::Error;

use crate::{bibtex, model::LoadResult};

pub const MAX_LIBRARY_BYTES: u64 = 64 * 1024 * 1024;
const SETTLE: Duration = Duration::from_millis(250);

/// Allowlisted diagnostics: never expose paths, contents or operating system errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SourceError {
    #[error("the library file is unavailable; keeping the last loaded references")]
    Unavailable,
    #[error("the library must be a regular file")]
    NotRegular,
    #[error("the library file is larger than 64 MiB")]
    TooLarge,
    #[error("the library file could not be read as UTF-8 text; keeping the last loaded references")]
    Unreadable,
    #[error("the library file contains an unfinished entry; keeping the last loaded references")]
    Incomplete,
    #[error("the library exceeded the load work limit; keeping the last loaded references")]
    LoadLimit,
    #[error("the library file is still changing; keeping the last loaded references")]
    Changing,
    #[error("automatic library updates are unavailable; use Reload to try again")]
    WatchUnavailable,
}

pub fn read_library_file(path: &Path) -> Result<String, SourceError> {
    let metadata = std::fs::metadata(path).map_err(|_| SourceError::Unavailable)?;
    if !metadata.is_file() {
        return Err(SourceError::NotRegular);
    }
    if metadata.len() > MAX_LIBRARY_BYTES {
        return Err(SourceError::TooLarge);
    }
    let file = std::fs::File::open(path).map_err(|_| SourceError::Unreadable)?;
    let mut text = String::new();
    file.take(MAX_LIBRARY_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|_| SourceError::Unreadable)?;
    if text.len() as u64 > MAX_LIBRARY_BYTES {
        return Err(SourceError::TooLarge);
    }
    Ok(text)
}

/// Initial loads remain tolerant of damaged entries. A reload additionally
/// refuses unclosed blocks, while duplicate keys and other warnings still load.
pub fn load_library_file(path: &Path, reload: bool) -> Result<LoadResult, SourceError> {
    parse(path, &read_library_file(path)?, reload)
}

fn parse(path: &Path, text: &str, reload: bool) -> Result<LoadResult, SourceError> {
    parse_with_budget(path, text, reload, bibtex::Budget::for_input(text.len()))
}

fn parse_with_budget(path: &Path, text: &str, reload: bool, budget: bibtex::Budget) -> Result<LoadResult, SourceError> {
    if reload && bibtex::has_incomplete_blocks(text) {
        return Err(SourceError::Incomplete);
    }
    let source = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let (result, limited) = bibtex::load_with_budget(&source.to_string_lossy(), text, budget);
    if reload && limited {
        return Err(SourceError::LoadLimit);
    }
    Ok(result)
}

/// Watches the containing directory so editor rename-replacements keep working.
/// Dropping stops the worker; a callback already running must still be guarded
/// by the caller's source-selection generation before installing its candidate.
pub struct SourceWatch {
    stopped: Arc<AtomicBool>,
    signal: SyncSender<()>,
}

impl SourceWatch {
    pub fn new(
        path: &Path,
        mut changed: impl FnMut(Result<LoadResult, SourceError>) + Send + 'static,
    ) -> Result<Self, SourceError> {
        // Keep the selected filename, rather than canonicalizing the file inode.
        let path = std::path::absolute(path).map_err(|_| SourceError::WatchUnavailable)?;
        let parent = path.parent().ok_or(SourceError::WatchUnavailable)?;
        let parent = std::fs::canonicalize(parent).map_err(|_| SourceError::WatchUnavailable)?;
        let path = parent.join(path.file_name().ok_or(SourceError::WatchUnavailable)?);
        let (signal, events) = mpsc::sync_channel(1);
        let failed = Arc::new(AtomicBool::new(false));
        let notify_failed = Arc::clone(&failed);
        let event_signal = signal.clone();
        let selected = path.clone();
        let target = Arc::new(Mutex::new(None::<PathBuf>));
        let event_target = Arc::clone(&target);
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let relevant = match event {
                Ok(event) => {
                    !matches!(event.kind, notify::EventKind::Access(_))
                        && (event.need_rescan()
                            || event.paths.iter().any(|p| {
                                p == &selected
                                    || p == &parent
                                    || event_target.lock().is_ok_and(|target| {
                                        target
                                            .as_ref()
                                            .is_some_and(|target| p == target || Some(p.as_path()) == target.parent())
                                    })
                            }))
                }
                Err(_) => {
                    notify_failed.store(true, Ordering::SeqCst);
                    true
                }
            };
            if relevant {
                let _ = event_signal.try_send(());
            }
        })
        .map_err(|_| SourceError::WatchUnavailable)?;
        watcher
            .watch(
                path.parent().ok_or(SourceError::WatchUnavailable)?,
                RecursiveMode::NonRecursive,
            )
            .map_err(|_| SourceError::WatchUnavailable)?;
        let mut target_parent = None;
        rearm_target(&mut watcher, &path, &target, &mut target_parent)?;
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&stopped);
        let retry = signal.clone();
        std::thread::Builder::new()
            .name("ibis-library-watch".into())
            .spawn(move || {
                // Also check once after registration to close the load/watch gap.
                while events.recv().is_ok() && !stop.load(Ordering::SeqCst) {
                    loop {
                        if stop.load(Ordering::SeqCst) {
                            return;
                        }
                        match events.recv_timeout(SETTLE) {
                            Ok(()) => continue,
                            Err(RecvTimeoutError::Disconnected) => return,
                            Err(RecvTimeoutError::Timeout) => break,
                        }
                    }
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    if failed.swap(false, Ordering::SeqCst) {
                        changed(Err(SourceError::WatchUnavailable));
                        continue;
                    }
                    // A selected symlink may now point into a different directory.
                    let candidate = rearm_target(&mut watcher, &path, &target, &mut target_parent)
                        .and_then(|()| stable_read(&path, &events, &stop));
                    if !stop.load(Ordering::SeqCst) {
                        if matches!(candidate, Err(SourceError::Changing)) {
                            let _ = retry.try_send(());
                        }
                        changed(candidate);
                    }
                }
            })
            .map_err(|_| SourceError::WatchUnavailable)?;
        let _ = signal.try_send(());
        Ok(Self { stopped, signal })
    }
}

/// Keep the selected path watched while following its resolved target as well.
/// If the target disappears, keep its parent registered so recreation is seen.
fn rearm_target(
    watcher: &mut notify::RecommendedWatcher,
    selected: &Path,
    target: &Mutex<Option<PathBuf>>,
    watched_parent: &mut Option<PathBuf>,
) -> Result<(), SourceError> {
    let resolved = std::fs::canonicalize(selected).ok().or_else(|| {
        // A dangling selected link can still name a file in an existing
        // directory. Resolve only that parent so creation of the leaf is seen.
        // One read_link plus OS canonicalization keeps resolution bounded.
        let linked = selected.parent()?.join(std::fs::read_link(selected).ok()?);
        Some(std::fs::canonicalize(linked.parent()?).ok()?.join(linked.file_name()?))
    });
    let Some(resolved) = resolved else {
        return Ok(());
    };
    let resolved = (resolved != selected).then_some(resolved);
    let parent = resolved
        .as_deref()
        .and_then(Path::parent)
        .filter(|parent| Some(*parent) != selected.parent())
        .map(Path::to_path_buf);
    if parent != *watched_parent {
        if let Some(parent) = &parent {
            watcher
                .watch(parent, RecursiveMode::NonRecursive)
                .map_err(|_| SourceError::WatchUnavailable)?;
        }
        if let Some(old) = watched_parent.as_deref() {
            watcher.unwatch(old).map_err(|_| SourceError::WatchUnavailable)?;
        }
        *watched_parent = parent;
    }
    *target.lock().map_err(|_| SourceError::WatchUnavailable)? = resolved;
    Ok(())
}

fn stable_read(path: &Path, events: &Receiver<()>, stopped: &AtomicBool) -> Result<LoadResult, SourceError> {
    let before = read_library_file(path)?;
    // A zero-length file and a valid prefix can both occur during a save.
    // Require identical observations separated by another quiet interval.
    match events.recv_timeout(SETTLE) {
        Ok(()) => return Err(SourceError::Changing),
        Err(RecvTimeoutError::Disconnected) => return Err(SourceError::WatchUnavailable),
        Err(RecvTimeoutError::Timeout) => {}
    }
    if stopped.load(Ordering::SeqCst) {
        return Err(SourceError::Changing);
    }
    let after = read_library_file(path)?;
    if before != after {
        return Err(SourceError::Changing);
    }
    parse(path, &after, true)
}

impl Drop for SourceWatch {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _ = self.signal.try_send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limited_load_keeps_completed_prefix_and_rejects_reload() {
        let fields = (0..100).map(|i| format!("f{i}={{v}}")).collect::<Vec<_>>().join(",");
        let text = format!("@xdata{{p,{fields}}}\n@misc{{good}}\n@misc{{bad,xdata={{p}}}}\n@misc{{suffix}}");
        let limited = |reload| {
            parse_with_budget(
                Path::new("synthetic.bib"),
                &text,
                reload,
                bibtex::Budget::new(32 * 1024),
            )
        };
        let initial = limited(false).unwrap_or_else(|e| panic!("{e}"));
        let keys: Vec<_> = initial.references.iter().map(|r| r.id.native_key.as_str()).collect();
        assert_eq!(keys, ["good"]);
        assert_eq!(initial.warnings.len(), 1);
        let warning = &initial.warnings[0];
        assert_eq!((warning.line, warning.key.as_deref()), (Some(3), Some("bad")));
        assert!(warning.message.contains("load work limit"));
        assert_eq!(limited(true).err(), Some(SourceError::LoadLimit));
    }
}
