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

use crate::{LoadedLibrary, attachments, bibtex, csl_json};
use ibis_core::contract::{LibraryId, LibraryObject, Snapshot};
use std::collections::BTreeMap;

pub const MAX_LIBRARY_BYTES: u64 = 64 * 1024 * 1024;
const SETTLE: Duration = Duration::from_millis(250);
const MAX_UNAVAILABLE_RETRY: Duration = Duration::from_secs(5);

/// Allowlisted diagnostics: never expose paths, contents or operating system errors.
/// Front ends add any context about their installed references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SourceError {
    #[error("the library file is unavailable")]
    Unavailable,
    #[error("the library must be a regular file")]
    NotRegular,
    #[error("the library file is larger than 64 MiB")]
    TooLarge,
    #[error("the library file is not UTF-8 text; save it as UTF-8")]
    Unreadable,
    #[error("the library file contains an unfinished entry")]
    Incomplete,
    #[error("the library exceeded the load work limit")]
    LoadLimit,
    #[error("the library file is still changing")]
    Changing,
    #[error("automatic library updates are unavailable; use Reload to try again")]
    WatchUnavailable,
    #[error("the library file does not contain valid reference data")]
    InvalidData,
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
    let file = retry_io(|| open_readonly(path), pause).map_err(|_| SourceError::Unavailable)?;
    read_open_file(file)
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
    path
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
pub(crate) fn load_library_file(path: &Path, reload: bool) -> Result<LoadedLibrary, SourceError> {
    parse(path, &read_library_file(path)?, reload, None)
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
pub fn load_file_snapshot(path: &Path) -> Result<(LoadedLibrary, SourceSnapshot), SourceError> {
    let text = read_library_file(path)?;
    let source = resolved_path(path).map_err(|_| SourceError::Unavailable)?;
    let loaded = parse(&source, &text, false, None)?;
    Ok((
        loaded,
        SourceSnapshot {
            source,
            text: text.into(),
        },
    ))
}

fn parse(path: &Path, text: &str, reload: bool, previous: Option<&Snapshot>) -> Result<LoadedLibrary, SourceError> {
    parse_with_budget(path, text, reload, bibtex::Budget::for_input(text.len()), previous)
}

fn parse_with_budget(
    path: &Path,
    text: &str,
    reload: bool,
    budget: bibtex::Budget,
    previous: Option<&Snapshot>,
) -> Result<LoadedLibrary, SourceError> {
    let source = resolved_path(path).map_err(|_| SourceError::Unavailable)?;
    let library = LibraryId::file(source.as_os_str().as_encoded_bytes());
    let text = text.trim_start_matches('\u{feff}');
    let extension = source.extension();
    let json = if extension.is_some_and(|extension| extension.eq_ignore_ascii_case("bib")) {
        false
    } else {
        extension.is_some_and(|extension| {
            extension.eq_ignore_ascii_case("json") || extension.eq_ignore_ascii_case("csljson")
        }) || text.trim_start().starts_with('[')
    };
    let (parsed, limited) = if json {
        let parsed = csl_json::load_str(&library, text)?;
        let limited = parsed
            .warnings
            .iter()
            .any(|warning| warning.code == crate::WarningCode::WorkLimit);
        (parsed, limited)
    } else {
        if reload && bibtex::has_incomplete_blocks(text) {
            return Err(SourceError::Incomplete);
        }
        bibtex::load_with_budget(&library, text, budget, previous)
    };
    if reload && limited {
        return Err(SourceError::LoadLimit);
    }
    let mut objects: Vec<_> = parsed.records.into_iter().map(LibraryObject::Record).collect();
    let mut warnings = parsed.warnings;
    let mut attachment_paths = BTreeMap::new();
    let attachment_limited = attachments::append(
        source.parent().ok_or(SourceError::Unavailable)?,
        parsed.attachments,
        &mut objects,
        &mut attachment_paths,
        &mut warnings,
    );
    if reload && attachment_limited {
        return Err(SourceError::LoadLimit);
    }
    let mut snapshot = match previous {
        Some(previous) => Snapshot::new_reusing(objects, previous, &parsed.reused),
        None => Snapshot::new(objects),
    }
    .map_err(|_| SourceError::InvalidData)?;
    if let Some(previous) = previous {
        snapshot.reuse_unchanged(previous);
    }
    Ok(LoadedLibrary {
        library,
        snapshot,
        warnings,
        attachment_paths,
        best_attachments: Default::default(),
    })
}

/// Tolerant initial read: damaged entries warn while complete entries load.
pub fn load_file(path: &Path) -> Result<LoadedLibrary, SourceError> {
    load_library_file(path, false)
}

/// Explicit refresh obeys the same two-identical-observations rule as watches.
pub fn reload_file(path: &Path) -> Result<LoadedLibrary, SourceError> {
    reload(path, None)
}

/// Reuse unchanged records while parsing, before a second CSL library accumulates.
pub fn reload_file_reusing(path: &Path, previous: &Snapshot) -> Result<LoadedLibrary, SourceError> {
    reload(path, Some(previous))
}

fn reload(path: &Path, previous: Option<&Snapshot>) -> Result<LoadedLibrary, SourceError> {
    let (sender, events) = mpsc::sync_channel(1);
    let stopped = AtomicBool::new(false);
    let mut snapshot = None;
    let result = stable_read(path, &events, &stopped, &mut snapshot, false, previous)?.ok_or(SourceError::Changing);
    drop(sender);
    result
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
        mut changed: impl FnMut(Result<LoadedLibrary, SourceError>) + Send + 'static,
    ) -> Result<Self, SourceError> {
        Self::start(path, None, None, move |candidate| {
            changed(candidate);
            None
        })
    }

    /// Check once after registration, but publish only if the source differs
    /// from the completed load. Ordinary notifications retain strict reloads.
    pub fn with_snapshot(
        path: &Path,
        snapshot: SourceSnapshot,
        mut changed: impl FnMut(Result<LoadedLibrary, SourceError>) + Send + 'static,
    ) -> Result<Self, SourceError> {
        Self::start(path, Some(snapshot), None, move |candidate| {
            changed(candidate);
            None
        })
    }

    /// Seed parsing with the already loaded model. The callback returns the
    /// source-scoped model it installed after coalescing, or None on rejection.
    pub fn with_library(
        path: &Path,
        observation: Option<SourceSnapshot>,
        model: Snapshot,
        changed: impl FnMut(Result<LoadedLibrary, SourceError>) -> Option<Snapshot> + Send + 'static,
    ) -> Result<Self, SourceError> {
        Self::start(path, observation, Some(model), changed)
    }

    fn start(
        path: &Path,
        mut snapshot: Option<SourceSnapshot>,
        mut model: Option<Snapshot>,
        mut changed: impl FnMut(Result<LoadedLibrary, SourceError>) -> Option<Snapshot> + Send + 'static,
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
                        let _ = changed(Err(SourceError::WatchUnavailable));
                        continue;
                    }
                    // A selected symlink may now point into a different directory.
                    let candidate = rearm_target(&mut watcher, &path, &target, &mut target_parent).and_then(|()| {
                        stable_read(&path, &events, &stop, &mut snapshot, error_reported, model.as_ref())
                    });
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
                                publish_candidate(&mut model, loaded, &mut changed);
                            }
                            Ok(None) => {}
                            Err(error) => {
                                error_reported = true;
                                let _ = changed(Err(error));
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

fn publish_candidate(
    model: &mut Option<Snapshot>,
    loaded: LoadedLibrary,
    changed: &mut impl FnMut(Result<LoadedLibrary, SourceError>) -> Option<Snapshot>,
) {
    if let Some(installed) = changed(Ok(loaded)) {
        *model = Some(installed);
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
    previous: Option<&Snapshot>,
) -> Result<Option<LoadedLibrary>, SourceError> {
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
            parse(&source, &after, false, previous).map(Some)
        } else {
            Ok(None)
        };
    }
    parse(&source, &after, true, previous).map(Some)
}

impl Drop for SourceWatch {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _ = self.signal.try_send(());
    }
}

fn open_readonly(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // A regular file can become a FIFO between path metadata and open.
        // Nonblocking open plus descriptor metadata prevents hanging on it.
        options.custom_flags(nix::fcntl::OFlag::O_NONBLOCK.bits());
    }
    options.open(path)
}
fn read_open_file(file: std::fs::File) -> Result<String, SourceError> {
    let metadata = file.metadata().map_err(|_| SourceError::Unavailable)?;
    if !metadata.is_file() {
        return Err(SourceError::NotRegular);
    }
    if metadata.len() > MAX_LIBRARY_BYTES {
        return Err(SourceError::TooLarge);
    }
    read_text(file)
}

#[cfg(all(test, unix))]
#[path = "../tests/support/subprocess_watchdog.rs"]
mod subprocess_watchdog;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn watcher_caches_only_the_accepted_coalesced_model() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("synthetic.bib");
        std::fs::write(&path, "@book{a,title={Keep}}\n@book{b,title={Before}}")
            .unwrap_or_else(|error| panic!("{error}"));
        let published = load_file(&path).unwrap_or_else(|error| panic!("{error}")).snapshot;
        let candidate = load_file(&path).unwrap_or_else(|error| panic!("{error}"));
        let discarded = Arc::downgrade(&candidate.snapshot.objects()[0]);
        let mut model = Some(Snapshot::new(vec![]).unwrap_or_else(|error| panic!("{error}")));
        publish_candidate(&mut model, candidate, &mut |result| {
            let mut loaded = result.unwrap_or_else(|error| panic!("{error}"));
            // Another selected owner already published the same library.
            loaded.snapshot.reuse_unchanged(&published);
            assert!(
                discarded.upgrade().is_none(),
                "the watcher retained a pre-publication payload"
            );
            Some(loaded.snapshot)
        });
        assert!(Arc::ptr_eq(
            &model.as_ref().unwrap_or_else(|| panic!("accepted model")).objects()[0],
            &published.objects()[0]
        ));
        std::fs::write(&path, "@book{a,title={Rejected}}").unwrap_or_else(|error| panic!("{error}"));
        let rejected = load_file(&path).unwrap_or_else(|error| panic!("{error}"));
        let rejected_payload = Arc::downgrade(&rejected.snapshot.objects()[0]);
        publish_candidate(&mut model, rejected, &mut |_| None);
        assert!(rejected_payload.upgrade().is_none());
        assert_eq!(model.as_ref(), Some(&published));
    }
    #[cfg(unix)]
    #[test]
    fn a_file_replaced_by_a_fifo_after_metadata_cannot_block() {
        const CHILD: &str = "IBIS_SOURCE_FIFO_CHILD";
        if std::env::var_os(CHILD).is_none() {
            subprocess_watchdog::assert_finishes(
                "file::tests::a_file_replaced_by_a_fifo_after_metadata_cannot_block",
                CHILD,
                "nonregular replacement blocked the source loader",
            );
            return;
        }
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = dir.path().join("synthetic.bib");
        std::fs::write(&path, "@book{synthetic}").unwrap_or_else(|error| panic!("{error}"));
        assert!(
            std::fs::metadata(&path)
                .unwrap_or_else(|error| panic!("{error}"))
                .is_file()
        );
        std::fs::remove_file(&path).unwrap_or_else(|error| panic!("{error}"));
        nix::unistd::mkfifo(&path, nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR)
            .unwrap_or_else(|error| panic!("{error}"));
        let file = open_readonly(&path).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(read_open_file(file), Err(SourceError::NotRegular));
    }
    #[test]
    fn exhausted_bibtex_work_keeps_prefix_but_refuses_reload() {
        let fields = (0..100)
            .map(|index| format!("field{index}={{value}}"))
            .collect::<Vec<_>>()
            .join(",");
        let text =
            format!("@xdata{{parent,{fields}}}\n@book{{good}}\n@book{{limited,xdata={{parent}}}}\n@book{{suffix}}");
        let parse_limited = |reload| {
            parse_with_budget(
                Path::new("synthetic.bib"),
                &text,
                reload,
                bibtex::Budget::new(32 * 1024),
                None,
            )
        };
        let initial = parse_limited(false).unwrap_or_else(|error| panic!("{error}"));
        let keys: Vec<_> = initial
            .snapshot
            .objects()
            .iter()
            .filter_map(|item| match &item.object {
                LibraryObject::Record(record) => record.citation_key().ok().flatten(),
                _ => None,
            })
            .collect();
        assert_eq!(keys, ["good"]);
        assert!(
            initial
                .warnings
                .iter()
                .any(|warning| warning.code == crate::WarningCode::WorkLimit)
        );
        assert_eq!(parse_limited(true).err(), Some(SourceError::LoadLimit));
    }
}
