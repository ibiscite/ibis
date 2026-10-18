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
const MAX_UNAVAILABLE_RETRY: Duration = Duration::from_secs(5);

/// Allowlisted diagnostics: never expose paths, contents or operating system errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SourceError {
    #[error("the library file is unavailable; keeping the last loaded references")]
    Unavailable,
    #[error("the library must be a regular file")]
    NotRegular,
    #[error("the library file is larger than 64 MiB")]
    TooLarge,
    #[error("the library file is not UTF-8 text; save it as UTF-8; keeping the last loaded references")]
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
    let pause = || std::thread::sleep(Duration::from_millis(50));
    let metadata = retry_io(|| std::fs::metadata(path), pause).map_err(|_| SourceError::Unavailable)?;
    if !metadata.is_file() {
        return Err(SourceError::NotRegular);
    }
    if metadata.len() > MAX_LIBRARY_BYTES {
        return Err(SourceError::TooLarge);
    }
    let file = retry_io(|| std::fs::File::open(path), pause).map_err(|_| SourceError::Unavailable)?;
    read_text(file)
}

fn retry_io<T>(mut operation: impl FnMut() -> std::io::Result<T>, mut pause: impl FnMut()) -> std::io::Result<T> {
    let mut remaining = 2;
    loop {
        match operation() {
            Err(error)
                if remaining > 0
                    && (matches!(error.kind(), std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock)
                    // Windows sharing/lock violations, not permanent access denial.
                    || cfg!(windows) && matches!(error.raw_os_error(), Some(32 | 33))) =>
            {
                remaining -= 1;
                pause();
            }
            result => return result,
        }
    }
}

fn read_text(file: impl Read) -> Result<String, SourceError> {
    let mut bytes = Vec::new();
    file.take(MAX_LIBRARY_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| SourceError::Unavailable)?;
    if bytes.len() as u64 > MAX_LIBRARY_BYTES {
        return Err(SourceError::TooLarge);
    }
    String::from_utf8(bytes).map_err(|_| SourceError::Unreadable)
}

fn resolved_path(path: &Path) -> std::io::Result<PathBuf> {
    canonical_path(path).or_else(|_| std::path::absolute(path).map(compatible_path))
}

fn canonical_path(path: &Path) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(path).map(compatible_path)
}

fn compatible_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        dunce::simplified(&path).to_path_buf()
    }
    #[cfg(not(windows))]
    {
        path
    }
}

// Canonicalize only the directory: editor replacement must keep selecting the
// filename. Preserve explicit verbatim semantics until the whole leaf is known;
// simplifying just its parent can make `library.bib.` select `library.bib`.
fn watched_path(path: &Path) -> std::io::Result<PathBuf> {
    let path = std::path::absolute(path)?;
    let parent = path.parent().ok_or(std::io::ErrorKind::InvalidInput)?;
    let parent = std::fs::canonicalize(parent).or_else(|_| std::path::absolute(parent))?;
    #[cfg(windows)]
    if matches!(path.components().next(), Some(std::path::Component::Prefix(prefix)) if prefix.kind().is_verbatim()) {
        return Ok(compatible_path(
            parent.join(path.file_name().ok_or(std::io::ErrorKind::InvalidInput)?),
        ));
    }
    Ok(compatible_path(parent).join(path.file_name().ok_or(std::io::ErrorKind::InvalidInput)?))
}

/// Initial loads remain tolerant of damaged entries. A reload additionally
/// refuses unclosed blocks, while duplicate keys and other warnings still load.
pub fn load_library_file(path: &Path, reload: bool) -> Result<LoadResult, SourceError> {
    parse(path, &read_library_file(path)?, reload)
}

/// The exact bytes and source identity seen by a completed load. Pass this to
/// `SourceWatch::with_snapshot` to avoid parsing an unchanged source again.
#[derive(Clone, PartialEq, Eq)]
pub struct SourceSnapshot {
    source: PathBuf,
    text: Arc<str>,
}

/// Load once and retain an observation for the watch-registration gap check.
/// This compares contents, so same-size edits with preserved timestamps remain
/// visible. Clones share the bounded source text rather than copying it.
pub fn load_library_snapshot(path: &Path, reload: bool) -> Result<(LoadResult, SourceSnapshot), SourceError> {
    let text = read_library_file(path)?;
    let source = resolved_path(path).map_err(|_| SourceError::Unavailable)?;
    let loaded = parse(&source, &text, reload)?;
    Ok((
        loaded,
        SourceSnapshot {
            source,
            text: text.into(),
        },
    ))
}

fn parse(path: &Path, text: &str, reload: bool) -> Result<LoadResult, SourceError> {
    parse_with_budget(path, text, reload, bibtex::Budget::for_input(text.len()))
}

fn parse_with_budget(path: &Path, text: &str, reload: bool, budget: bibtex::Budget) -> Result<LoadResult, SourceError> {
    if reload && bibtex::has_incomplete_blocks(text) {
        return Err(SourceError::Incomplete);
    }
    let source = resolved_path(path).map_err(|_| SourceError::Unavailable)?;
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
        changed: impl FnMut(Result<LoadResult, SourceError>) + Send + 'static,
    ) -> Result<Self, SourceError> {
        Self::start(path, None, changed)
    }

    /// Check once after registration, but publish only if the source differs
    /// from the completed load. Ordinary notifications retain strict reloads.
    pub fn with_snapshot(
        path: &Path,
        snapshot: SourceSnapshot,
        changed: impl FnMut(Result<LoadResult, SourceError>) + Send + 'static,
    ) -> Result<Self, SourceError> {
        Self::start(path, Some(snapshot), changed)
    }

    fn start(
        path: &Path,
        mut snapshot: Option<SourceSnapshot>,
        mut changed: impl FnMut(Result<LoadResult, SourceError>) + Send + 'static,
    ) -> Result<Self, SourceError> {
        // Keep the selected filename, rather than canonicalizing the file inode.
        let path = watched_path(path).map_err(|_| SourceError::WatchUnavailable)?;
        let parent = path.parent().ok_or(SourceError::WatchUnavailable)?.to_path_buf();
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
                let mut error_reported = false;
                let mut unavailable_retry = None;
                while wait_for_change(&events, unavailable_retry) && !stop.load(Ordering::SeqCst) {
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
                        unavailable_retry = None;
                        error_reported = true;
                        changed(Err(SourceError::WatchUnavailable));
                        continue;
                    }
                    // A selected symlink may now point into a different directory.
                    let candidate = rearm_target(&mut watcher, &path, &target, &mut target_parent)
                        .and_then(|()| stable_read(&path, &events, &stop, &mut snapshot, error_reported));
                    unavailable_retry = if matches!(candidate, Err(SourceError::Unavailable)) {
                        Some(unavailable_retry.map_or(SETTLE, |delay| (delay * 2).min(MAX_UNAVAILABLE_RETRY)))
                    } else {
                        None
                    };
                    if !stop.load(Ordering::SeqCst) {
                        if matches!(candidate, Err(SourceError::Changing)) {
                            let _ = retry.try_send(());
                        }
                        match candidate {
                            Ok(Some(loaded)) => {
                                error_reported = false;
                                changed(Ok(loaded));
                            }
                            Ok(None) => {}
                            Err(error) => {
                                error_reported = true;
                                changed(Err(error));
                            }
                        }
                    }
                }
            })
            .map_err(|_| SourceError::WatchUnavailable)?;
        let _ = signal.try_send(());
        Ok(Self { stopped, signal })
    }
}

// A failed read may outlast the short I/O retries without generating a later
// notification (for example a Windows editor's exclusive handle). Keep retrying
// with a capped delay; file events and dropping the watcher interrupt the wait.
fn wait_for_change(events: &Receiver<()>, unavailable_retry: Option<Duration>) -> bool {
    match unavailable_retry {
        Some(delay) => !matches!(events.recv_timeout(delay), Err(RecvTimeoutError::Disconnected)),
        None => events.recv().is_ok(),
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
    let resolved = canonical_path(selected).ok().or_else(|| {
        // A dangling selected link can still name a file in an existing
        // directory. Resolve only that parent so creation of the leaf is seen.
        // One read_link plus OS canonicalization keeps resolution bounded.
        let linked = selected.parent()?.join(std::fs::read_link(selected).ok()?);
        Some(canonical_path(linked.parent()?).ok()?.join(linked.file_name()?))
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

fn stable_read(
    path: &Path,
    events: &Receiver<()>,
    stopped: &AtomicBool,
    snapshot: &mut Option<SourceSnapshot>,
    error_reported: bool,
) -> Result<Option<LoadResult>, SourceError> {
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
    let source = resolved_path(path).map_err(|_| SourceError::Unavailable)?;
    if snapshot
        .take()
        .is_some_and(|previous| previous.source == source && previous.text.as_ref() == after)
    {
        // An earlier gap error must clear even if the file returns unchanged.
        // Only these exact initially accepted bytes retain tolerant loading.
        return if error_reported {
            parse(&source, &after, false).map(Some)
        } else {
            Ok(None)
        };
    }
    parse(&source, &after, true).map(Some)
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
    fn temporary_open_errors_retry_but_attempts_remain_bounded() {
        for failures in [2, 10] {
            let (mut attempts, mut pauses) = (0, 0);
            let result = retry_io(
                || {
                    attempts += 1;
                    if attempts <= failures {
                        Err(std::io::ErrorKind::WouldBlock.into())
                    } else {
                        Ok(7)
                    }
                },
                || pauses += 1,
            );
            assert_eq!(attempts, 3);
            assert_eq!(pauses, 2);
            if failures == 2 {
                assert_eq!(result.ok(), Some(7));
            } else {
                assert!(result.is_err());
            }
        }
        let mut pauses = 0;
        let result: std::io::Result<()> = retry_io(|| Err(std::io::ErrorKind::PermissionDenied.into()), || pauses += 1);
        assert!(result.is_err());
        assert_eq!(pauses, 0);
    }

    #[cfg(windows)]
    #[test]
    fn an_exclusive_windows_lock_recovers_on_the_next_attempt() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = dir.path().join("synthetic.bib");
        std::fs::write(&path, "@misc{k}").unwrap_or_else(|e| panic!("{e}"));
        let mut lock = Some(
            std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(&path)
                .unwrap_or_else(|e| panic!("{e}")),
        );
        let mut pauses = 0;
        let opened = retry_io(
            || std::fs::File::open(&path),
            || {
                pauses += 1;
                drop(lock.take());
            },
        );
        assert!(opened.is_ok());
        assert_eq!(pauses, 1);
    }

    #[cfg(windows)]
    #[test]
    fn ordinary_and_verbatim_windows_paths_keep_the_same_identity() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = dir.path().join("références synthetic.bib");
        std::fs::write(&path, "@misc{k}").unwrap_or_else(|e| panic!("{e}"));
        let verbatim = std::fs::canonicalize(&path).unwrap_or_else(|e| panic!("{e}"));
        let expected = dunce::canonicalize(&path).unwrap_or_else(|e| panic!("{e}"));
        for spelling in [&path, &verbatim] {
            let loaded = load_library_file(spelling, false).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(loaded.references[0].id.source, expected.to_string_lossy());
        }
        // Simulate canonicalization becoming unavailable after a successful read.
        let missing = verbatim.with_file_name("missing.bib");
        let loaded = parse(&missing, "@misc{k}", false).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            loaded.references[0].id.source,
            expected.with_file_name("missing.bib").to_string_lossy()
        );
    }

    #[test]
    fn read_failures_are_distinct_from_non_utf8_libraries() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::PermissionDenied.into())
            }
        }
        assert_eq!(read_text(Broken), Err(SourceError::Unavailable));
        for bytes in [b"caf\xe9".as_slice(), b"\xff\xfe@\0m\0".as_slice()] {
            assert_eq!(read_text(bytes), Err(SourceError::Unreadable));
        }
        assert_eq!(
            read_text("Synthetic café".as_bytes()).ok().as_deref(),
            Some("Synthetic café")
        );
    }

    #[test]
    fn canonicalization_failure_keeps_an_absolute_record_identity() {
        let path = Path::new("missing-synthetic-library/library.bib");
        let loaded = parse(path, "@misc{k}", false).unwrap_or_else(|e| panic!("{e}"));
        let absolute = std::path::absolute(path).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(loaded.references[0].id.source, absolute.to_string_lossy());
        assert_eq!(watched_path(path).unwrap_or_else(|e| panic!("{e}")), absolute);
    }

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
