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

use crate::{LoadedLibrary, attachments, bibtex, csl_json, exported};
use ibis_core::contract::{Fingerprint, LibraryObject, Snapshot};

mod io;
mod watch;

#[cfg(feature = "endnote")]
pub(crate) use io::read_library_file_sync;

pub const MAX_LIBRARY_BYTES: u64 = 64 * 1024 * 1024;
const SETTLE: Duration = Duration::from_millis(250);

/// Native discovery and health share file reads' eight-call limit and two-second wait.
pub fn inspect_io<T: Send + 'static>(operation: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    io::bounded(move || Ok(operation())).ok()
}

/// Allowlisted diagnostics: never expose paths, contents or operating system errors.
/// Front ends add any context about their installed references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SourceError {
    #[error("the library load was cancelled")]
    Cancelled,
    #[error("the library file was moved, renamed or removed")]
    Missing,
    #[error("the library file is unavailable; download any online-only file to this computer first")]
    Unavailable,
    #[error("the library must be a regular file")]
    NotRegular,
    #[error("the library file is larger than 64 MiB")]
    TooLarge,
    #[error("the library file is not UTF-8 text; save it as UTF-8")]
    Unreadable,
    #[error("the library file contains an unfinished entry")]
    Incomplete,
    #[error("the library file is still changing")]
    Changing,
    #[error("automatic library updates are unavailable; use Reload to try again")]
    WatchUnavailable,
    #[error("the library file does not contain valid reference data")]
    InvalidData,
    #[error("the CSV file's first row does not name the template's columns")]
    CsvTemplate,
    #[error("the file is not in a format Ibis reads")]
    UnknownFormat,
    #[error("the attachment file is outside the library's folder; Settings › AI Apps can allow it")]
    AttachmentOutsideLibrary,
}

fn file_io_error(error: std::io::Error) -> SourceError {
    if error.kind() == std::io::ErrorKind::NotFound {
        SourceError::Missing
    } else {
        SourceError::Unavailable
    }
}

/// What a library file holds, from its name or reference entry structure.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    Bibtex,
    CslJson,
    Ris,
    Csv,
}

/// Each format with the file names it goes by: the one place a format is named.
/// Front ends take their chooser and drop lists from here. EndNote online saves
/// RIS as `.txt`, but that name alone does not say a file is a library.
const NAMES: [(Format, &[&str]); 4] = [
    (Format::Bibtex, &["bib", "bibtex", "biblatex"]),
    (Format::CslJson, &["json", "csljson"]),
    (Format::Ris, &["ris", "txt"]),
    (Format::Csv, &["csv"]),
];

impl Format {
    /// By name first: BibTeX comments may start with a JSON-looking bracket.
    /// An unknown name needs a RIS opening or a BibTeX entry block.
    pub(crate) fn of(path: &Path, text: &str) -> Result<Self, SourceError> {
        match Self::named(path) {
            Some(format) => Ok(format),
            None if crate::exported::ris::looks_like_ris(text) => Ok(Self::Ris),
            None if bibtex::has_entry_block(text) => Ok(Self::Bibtex),
            None => Err(SourceError::UnknownFormat),
        }
    }

    /// What a library in this format offers. A proposal rewrites the file at
    /// the path the caller named, so that name must say BibTeX too: never a
    /// file recognised only by its text, or a link whose own name says another
    /// format.
    fn capabilities(self, asked: &Path) -> &'static [crate::SourceCapability] {
        use crate::SourceCapability::*;
        match self {
            Self::Bibtex if Self::named(asked) == Some(Self::Bibtex) => {
                &[BibliographicRecords, NativeCitationKeys, AttachmentText, Proposals]
            }
            Self::Bibtex | Self::CslJson => &[BibliographicRecords, NativeCitationKeys, AttachmentText],
            // Decision 0043: no citation keys and no attachments.
            Self::Ris | Self::Csv => &[BibliographicRecords],
        }
    }

    pub(crate) fn named(path: &Path) -> Option<Self> {
        NAMES.iter().find_map(|(format, names)| {
            path.extension()
                .is_some_and(|extension| names.iter().any(|name| extension.eq_ignore_ascii_case(name)))
                .then_some(*format)
        })
    }
}

/// Every file name ending a library file chooser offers.
pub fn library_file_extensions() -> impl Iterator<Item = &'static str> {
    NAMES.iter().flat_map(|(_, names)| names.iter().copied())
}

/// Whether a file's name alone says it is a library, as a dropped file's must.
pub fn names_a_library(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        !extension.eq_ignore_ascii_case("txt")
            && library_file_extensions().any(|name| extension.eq_ignore_ascii_case(name))
    })
}

fn read_library_file(path: &Path, scope: &io::Scope<'_>) -> Result<String, SourceError> {
    let path = path.to_path_buf();
    scope.run(move || io::read_library_file_sync(&path))
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

/// `expected` is the size the file reported; reading into exactly that much
/// keeps a large library from briefly costing twice its size.
fn read_text(file: impl Read, expected: u64) -> Result<String, SourceError> {
    let mut bytes = Vec::with_capacity(usize::try_from(expected.min(MAX_LIBRARY_BYTES)).unwrap_or(0));
    file.take(MAX_LIBRARY_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| SourceError::Unavailable)?;
    if bytes.len() as u64 > MAX_LIBRARY_BYTES {
        return Err(SourceError::TooLarge);
    }
    String::from_utf8(bytes).map_err(|_| SourceError::Unreadable)
}

fn canonical_path(path: &Path, scope: &io::Scope<'_>) -> Result<PathBuf, SourceError> {
    let path = path.to_path_buf();
    scope.run(move || canonical_path_sync(&path).map_err(file_io_error))
}

// Only call directly from a job already protected by the filesystem deadline.
fn canonical_path_sync(path: &Path) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(path)
}

// Canonicalize only the directory: editor replacement must keep selecting the
// filename. Preserve explicit verbatim semantics until the whole leaf is known;
// simplifying just its parent can make `library.bib.` select `library.bib`.
fn watched_path(path: &Path) -> std::io::Result<PathBuf> {
    let path = std::path::absolute(path)?;
    let parent = path.parent().ok_or(std::io::ErrorKind::InvalidInput)?;
    let parent = canonical_path(parent, &io::Scope::new(&|| false)).or_else(|_| std::path::absolute(parent))?;
    #[cfg(windows)]
    if matches!(path.components().next(), Some(std::path::Component::Prefix(prefix)) if prefix.kind().is_verbatim()) {
        return Ok(parent.join(path.file_name().ok_or(std::io::ErrorKind::InvalidInput)?));
    }
    Ok(parent.join(path.file_name().ok_or(std::io::ErrorKind::InvalidInput)?))
}

/// The exact bytes and source identity seen by a completed load. Pass this to
/// `SourceWatch::with_snapshot` to avoid parsing an unchanged source again.
#[derive(Clone, PartialEq, Eq)]
pub struct SourceSnapshot {
    source: PathBuf,
    // Shares the buffer that was read; `Arc<str>` would copy all of it.
    text: Arc<String>,
}

impl SourceSnapshot {
    pub(crate) fn text(&self) -> Arc<String> {
        self.text.clone()
    }

    /// What two observations compare, without keeping the text alive.
    fn seen(self) -> Seen {
        Seen {
            digest: Fingerprint::of_bytes(self.text.as_bytes()),
            source: self.source,
        }
    }
}

/// An earlier observation, remembered only to compare with a later one: a
/// full copy of a large library per observation would multiply its cost.
#[derive(PartialEq, Eq)]
struct Seen {
    source: PathBuf,
    digest: Fingerprint,
}

/// Load once and retain an observation for the watch-registration gap check.
/// This compares contents, so same-size edits with preserved timestamps remain
/// visible. Clones share the bounded source text rather than copying it.
pub fn load_file_snapshot(
    path: &Path,
    cancelled: &impl Fn() -> bool,
) -> Result<(LoadedLibrary, SourceSnapshot), SourceError> {
    load_file_snapshot_with(path, || {}, &io::Scope::new(cancelled))
}

fn load_file_snapshot_with(
    path: &Path,
    after_read: impl FnOnce(),
    scope: &io::Scope<'_>,
) -> Result<(LoadedLibrary, SourceSnapshot), SourceError> {
    let observation = SourceSnapshot::read_with(path, after_read, scope)?;
    let loaded = parse(path, &observation, None, scope)?;
    Ok((loaded, observation))
}

impl SourceSnapshot {
    /// Private canonical identity of the same observation that supplied the bytes.
    pub fn source_path(&self) -> &Path {
        &self.source
    }

    fn read_with(path: &Path, after_read: impl FnOnce(), scope: &io::Scope<'_>) -> Result<Self, SourceError> {
        let source = canonical_path(path, scope)?;
        let text = read_library_file(&source, scope)?;
        after_read();
        let current = canonical_path(path, scope);
        if current == Err(SourceError::Cancelled) {
            return Err(SourceError::Cancelled);
        }
        if current.ok().as_ref() != Some(&source) {
            return Err(SourceError::Changing);
        }
        Ok(Self {
            source,
            text: Arc::new(text),
        })
    }
}

/// Strict Reload carries the second of two identical source observations.
pub fn reload_file_snapshot(
    path: &Path,
    previous: Option<&Snapshot>,
    cancelled: &impl Fn() -> bool,
) -> Result<(LoadedLibrary, SourceSnapshot), SourceError> {
    let scope = io::Scope::new(cancelled);
    let observation = stable_observation_with(path, || std::thread::sleep(SETTLE), &scope)?;
    let loaded = parse(path, &observation, previous, &scope)?;
    Ok((loaded, observation))
}

/// Two identical reads, so a file an editor is still writing is never trusted.
pub(crate) fn stable_observation(path: &Path) -> Result<SourceSnapshot, SourceError> {
    stable_observation_with(path, || std::thread::sleep(SETTLE), &io::Scope::new(&|| false))
}

/// A file read as a library without becoming one, tolerating damaged entries.
pub(crate) fn parse_for_project(path: &Path, text: &str) -> Result<LoadedLibrary, SourceError> {
    parse_with_budget(path, path, text, bibtex::Budget::for_input(text.len()), None, None)
}

/// `key`'s original entry in the file `item` came from, when the file still
/// holds exactly what the library read, by `cite` fingerprint.
pub fn original_entry(
    path: &Path,
    item: &ibis_core::contract::ItemRef,
    key: &str,
    cite: &ibis_core::contract::Fingerprint,
) -> Result<String, crate::project::EntryRefusal> {
    use crate::project::EntryRefusal;
    // Only BibTeX has an entry to copy; most other files say so by name.
    if Format::named(path).is_some_and(|format| format != Format::Bibtex) {
        return Err(EntryRefusal::NoOriginal);
    }
    let observation = stable_observation(path).map_err(|_| EntryRefusal::Changed)?;
    if Format::of(path, &observation.text) != Ok(Format::Bibtex) {
        return Err(EntryRefusal::NoOriginal);
    }
    // Parsed from the canonical path, as the library was, so the item's ID matches.
    let loaded = parse_for_project(&observation.source, &observation.text).map_err(|_| EntryRefusal::Changed)?;
    let current = loaded.snapshot.get(item).ok_or(EntryRefusal::Changed)?;
    if current.fingerprints.cite != *cite {
        return Err(EntryRefusal::Changed);
    }
    crate::project::entry_in(&observation.text, key)
}

fn stable_observation_with(
    path: &Path,
    settle: impl FnOnce(),
    scope: &io::Scope<'_>,
) -> Result<SourceSnapshot, SourceError> {
    let before = SourceSnapshot::read_with(path, || {}, scope)?.seen();
    settle();
    let after = SourceSnapshot::read_with(path, || {}, scope)?;
    if before != after.clone().seen() {
        return Err(SourceError::Changing);
    }
    Ok(after)
}

/// One read policy: the same bytes give the same library whether opened or
/// reloaded. Damaged entries and work limits warn while the rest loads; a
/// stable read has already ruled out a file still being written. `asked` is
/// the path the caller named, which is the one any write goes to.
fn parse(
    asked: &Path,
    observation: &SourceSnapshot,
    previous: Option<&Snapshot>,
    scope: &io::Scope<'_>,
) -> Result<LoadedLibrary, SourceError> {
    let text = &observation.text;
    let budget = bibtex::Budget::for_input(text.len());
    parse_with_budget(&observation.source, asked, text, budget, previous, Some(scope))
}

fn parse_with_budget(
    path: &Path,
    asked: &Path,
    text: &str,
    budget: bibtex::Budget,
    previous: Option<&Snapshot>,
    scope: Option<&io::Scope<'_>>,
) -> Result<LoadedLibrary, SourceError> {
    let (source, library, format, parsed) = parse_format(path, text, budget, previous)?;
    let parsed_cut_short = parsed.cut_short;
    let mut objects: Vec<_> = parsed.records.into_iter().map(LibraryObject::Record).collect();
    let mut warnings = parsed.warnings;
    let folder = crate::folder::is_folder_library(&source);
    if let Some(scope) = scope {
        let references = attachments::append(parsed.attachments, &mut objects, &mut warnings);
        // A folder library says when its folder holds PDFs it has not kept.
        if folder && let Some(dir) = source.parent() {
            let kept = crate::folder::named(references.iter().map(|(_, location)| location.clone()));
            let dir = dir.to_path_buf();
            let listing = io::bounded(move || crate::folder::list(&dir, &|| false)).ok();
            if listing.is_some_and(|listing| !crate::folder::unkept(&listing, &kept).is_empty()) {
                warnings.push(crate::SourceWarning::at(crate::WarningCode::PdfsNotKept, None));
            }
        }
        scope.attachments(
            &mut objects,
            source.parent().ok_or(SourceError::Unavailable)?,
            references,
        )?;
    }
    let mut snapshot = match previous {
        Some(previous) => Snapshot::new_reusing(objects, previous, &parsed.reused),
        None => Snapshot::new(objects),
    }
    .map_err(|_| SourceError::InvalidData)?;
    if let Some(previous) = previous {
        snapshot.reuse_unchanged(previous);
    }
    let (kind, capabilities) = if folder {
        (crate::LibraryKind::Folder, crate::SourceCapability::FOLDER)
    } else {
        (crate::LibraryKind::File, format.capabilities(asked))
    };
    Ok(LoadedLibrary {
        library,
        kind,
        capabilities,
        snapshot,
        warnings,
        duplicate_keys: parsed.duplicate_keys,
        cut_short: parsed_cut_short,
        damaged_keys: parsed.damaged_keys,
    })
}

/// The file's own records and attachments, before anything is probed.
fn parse_format(
    path: &Path,
    text: &str,
    budget: bibtex::Budget,
    previous: Option<&Snapshot>,
) -> Result<(PathBuf, ibis_core::contract::LibraryId, Format, crate::ParsedLibrary), SourceError> {
    let source = std::path::absolute(path).map_err(|_| SourceError::Unavailable)?;
    let library = if crate::folder::is_folder_library(&source) {
        crate::folder::library_id(&source)
    } else {
        crate::identity::library_id(crate::LibraryKind::File, &[source.as_os_str().as_encoded_bytes()])
    };
    let text = text.trim_start_matches('\u{feff}');
    let format = Format::of(&source, text)?;
    let parsed = match format {
        Format::CslJson => csl_json::load_str(&library, text)?,
        Format::Ris => exported::ris::load(&library, text, previous)?,
        Format::Csv => exported::csv::load(&library, text, previous)?,
        Format::Bibtex => bibtex::load_with_budget(&library, text, budget, previous),
    };
    Ok((source, library, format, parsed))
}

/// Where the attachment `key` of `parent` is, when the library file at
/// `path` still holds that reference as the library read it (`full` is its
/// fingerprint). Links are resolved to check the library-folder boundary;
/// `outside_library` explicitly allows files beyond it. `None` when the file
/// has no such attachment or its location is missing or not a local file.
pub fn attachment_file(
    path: &Path,
    parent: &ibis_core::contract::ItemRef,
    full: &Fingerprint,
    key: &str,
    outside_library: bool,
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<PathBuf>, SourceError> {
    attachment_files(path, &[(parent, full)], outside_library, cancelled)?
        .pop()
        .into_iter()
        .flatten()
        .find(|(found, _)| found == key)
        .map_or(Ok(None), |(_, file)| file)
}

/// [`attachment_file`] for every attachment of each of `parents`, reading
/// the library file once: for each parent, in order, its attachments' keys
/// and where each is, or why it is refused.
#[allow(
    clippy::type_complexity,
    reason = "One answer per attachment, each with its own refusal."
)]
pub fn attachment_files(
    path: &Path,
    parents: &[(&ibis_core::contract::ItemRef, &Fingerprint)],
    outside_library: bool,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<Vec<(String, Result<Option<PathBuf>, SourceError>)>>, SourceError> {
    let observation = SourceSnapshot::read_with(path, || {}, &io::Scope::new(cancelled))?;
    let (source, library, _, parsed) = parse_format(
        &observation.source,
        &observation.text,
        bibtex::Budget::for_input(observation.text.len()),
        None,
    )?;
    if parents.iter().any(|(parent, _)| library != parent.library) {
        return Err(SourceError::Changing);
    }
    let mut objects: Vec<_> = parsed.records.into_iter().map(LibraryObject::Record).collect();
    let references = attachments::append(parsed.attachments, &mut objects, &mut Vec::new());
    let located: Vec<_> = references
        .into_iter()
        .filter_map(|(index, location)| match &objects[index] {
            LibraryObject::Attachment(attachment) => {
                Some((attachment.parent.clone()?, attachment.id.key.clone(), location))
            }
            _ => None,
        })
        .collect();
    let snapshot = Snapshot::new(objects).map_err(|_| SourceError::InvalidData)?;
    if parents.iter().any(|(parent, full)| {
        snapshot
            .get(parent)
            .is_none_or(|current| current.fingerprints.full != **full)
    }) {
        return Err(SourceError::Changing);
    }
    let base = source.parent().ok_or(SourceError::Unavailable)?;
    Ok(parents
        .iter()
        .map(|(parent, _)| {
            located
                .iter()
                .filter(|(owner, _, _)| owner == *parent)
                .map(|(_, key, location)| (key.clone(), attachment_path(base, Path::new(location), outside_library)))
                .collect()
        })
        .collect())
}

/// A file field's location beside its library. On Windows the canonical
/// library folder is a `\\?\C:\` path, where `/` is no separator and which
/// the local-file checks refuse, so it becomes the plain `C:\` form first.
pub(crate) fn attachment_path(
    base: &Path,
    location: &Path,
    outside_library: bool,
) -> Result<Option<PathBuf>, SourceError> {
    let path = if location.is_absolute() {
        location.to_path_buf()
    } else {
        let mut path = plain_disk(base);
        for part in location.components() {
            // Windows resolves `..` by name before the file system sees it,
            // and its local-file checks take only that plain form.
            #[cfg(windows)]
            if part == std::path::Component::ParentDir {
                path.pop();
                continue;
            }
            path.push(part);
        }
        path
    };
    if outside_library {
        return Ok(Some(path));
    }
    let base = base.to_path_buf();
    io::bounded(move || {
        let resolved = match canonical_path_sync(&path) {
            Ok(resolved) => resolved,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(file_io_error(error)),
        };
        if !resolved.starts_with(canonical_path_sync(&base).map_err(file_io_error)?) {
            return Err(SourceError::AttachmentOutsideLibrary);
        }
        // Keep the original path: the PDF opener still refuses links and
        // checks every component without following them.
        Ok(Some(path))
    })
}

#[cfg(windows)]
fn plain_disk(path: &Path) -> PathBuf {
    use std::path::{Component, Prefix};
    let mut parts = path.components();
    if let Some(Component::Prefix(prefix)) = parts.next()
        && let Prefix::VerbatimDisk(letter) = prefix.kind()
    {
        let mut plain = PathBuf::from(format!("{}:\\", char::from(letter)));
        for part in parts.filter(|part| !matches!(part, Component::RootDir)) {
            plain.push(part);
        }
        return plain;
    }
    path.to_path_buf()
}

#[cfg(not(windows))]
fn plain_disk(path: &Path) -> PathBuf {
    path.to_path_buf()
}

/// One read: damaged entries warn while complete entries load.
pub fn load_file(path: &Path) -> Result<LoadedLibrary, SourceError> {
    load_file_snapshot(path, &|| false).map(|(loaded, _)| loaded)
}

/// Explicit refresh obeys the same two-identical-observations rule as watches.
pub fn reload_file(path: &Path) -> Result<LoadedLibrary, SourceError> {
    reload_file_snapshot(path, None, &|| false).map(|(loaded, _)| loaded)
}

/// Reuse unchanged records while parsing, before a second CSL library accumulates.
pub fn reload_file_reusing(path: &Path, previous: &Snapshot) -> Result<LoadedLibrary, SourceError> {
    reload_file_snapshot(path, Some(previous), &|| false).map(|(loaded, _)| loaded)
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
    /// from the completed load. Ordinary notifications reload after two identical reads.
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
        snapshot: Option<SourceSnapshot>,
        model: Option<Snapshot>,
        changed: impl FnMut(Result<LoadedLibrary, SourceError>) -> Option<Snapshot> + Send + 'static,
    ) -> Result<Self, SourceError> {
        Self::start_with_policy(path, snapshot, model, changed, io::supports_notifications)
    }

    fn start_with_policy(
        path: &Path,
        snapshot: Option<SourceSnapshot>,
        mut model: Option<Snapshot>,
        mut changed: impl FnMut(Result<LoadedLibrary, SourceError>) -> Option<Snapshot> + Send + 'static,
        supports_notifications: impl FnOnce(&Path) -> bool,
    ) -> Result<Self, SourceError> {
        if !supports_notifications(path) {
            return Err(SourceError::WatchUnavailable);
        }
        // Keep the selected filename, rather than canonicalizing the file inode.
        let path = watched_path(path).map_err(|_| SourceError::WatchUnavailable)?;
        let (signal, events) = mpsc::sync_channel(1);
        let failed = Arc::new(AtomicBool::new(false));
        let notify_failed = Arc::clone(&failed);
        let dirty = Arc::new(AtomicBool::new(false));
        let notify_dirty = Arc::clone(&dirty);
        let event_signal = signal.clone();
        let selected = path.clone();
        let target = Arc::new(Mutex::new(None::<PathBuf>));
        let event_target = Arc::clone(&target);
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let relevant = match event {
                Ok(event) => {
                    let Ok(target) = event_target.lock() else {
                        notify_failed.store(true, Ordering::SeqCst);
                        notify_dirty.store(true, Ordering::SeqCst);
                        let _ = event_signal.try_send(());
                        return;
                    };
                    let (relevant, reset) = watch::invalidation(&event, &selected, target.as_deref());
                    if reset {
                        notify_dirty.store(true, Ordering::SeqCst);
                    }
                    relevant
                }
                Err(_) => {
                    notify_failed.store(true, Ordering::SeqCst);
                    notify_dirty.store(true, Ordering::SeqCst);
                    true
                }
            };
            if relevant {
                let _ = event_signal.try_send(());
            }
        })
        .map_err(|_| SourceError::WatchUnavailable)?;
        let mut directories = Default::default();
        watch::rearm(&mut watcher, &path, &target, &mut directories, true)?;
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&stopped);
        let retry = signal.clone();
        std::thread::Builder::new()
            .name("ibis-library-watch".into())
            .spawn(move || {
                // Hashed here, not on the opening thread; the text it frees is
                // a whole library that the gap check below does not need.
                let mut snapshot = snapshot.map(SourceSnapshot::seen);
                // Also check once after registration to close the load/watch gap.
                let mut error_reported = false;
                // Windows reports one delete as several directory events: the
                // same failure is one report until a read succeeds again.
                let mut reported = None;
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
                        error_reported = true;
                        let _ = changed(Err(SourceError::WatchUnavailable));
                        continue;
                    }
                    // A selected symlink may now point into a different directory.
                    let candidate = if !io::supports_notifications(&path) {
                        Err(SourceError::WatchUnavailable)
                    } else {
                        watch::rearm(
                            &mut watcher,
                            &path,
                            &target,
                            &mut directories,
                            dirty.swap(false, Ordering::SeqCst),
                        )
                        .and_then(|()| {
                            stable_read(&path, &events, &stop, &mut snapshot, error_reported, model.as_ref())
                        })
                    };
                    if matches!(candidate, Err(SourceError::WatchUnavailable)) {
                        // A later leaf event must retry uncertain registrations,
                        // rather than trusting their cached path membership.
                        dirty.store(true, Ordering::SeqCst);
                    }
                    if !stop.load(Ordering::SeqCst) {
                        if matches!(candidate, Err(SourceError::Changing)) {
                            let _ = retry.try_send(());
                        }
                        match candidate {
                            Ok(Some(loaded)) => {
                                error_reported = false;
                                reported = None;
                                publish_candidate(&mut model, loaded, &mut changed);
                            }
                            Ok(None) => {}
                            Err(error) => {
                                error_reported = true;
                                if reported != Some(error) {
                                    reported = Some(error);
                                    let _ = changed(Err(error));
                                }
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

fn stable_read(
    path: &Path,
    events: &Receiver<()>,
    stopped: &AtomicBool,
    snapshot: &mut Option<Seen>,
    error_reported: bool,
    previous: Option<&Snapshot>,
) -> Result<Option<LoadedLibrary>, SourceError> {
    let cancelled = || stopped.load(Ordering::SeqCst);
    let scope = &io::Scope::new(&cancelled);
    let before = SourceSnapshot::read_with(path, || {}, scope)?.seen();
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
    let after = SourceSnapshot::read_with(path, || {}, scope)?;
    let seen = after.clone().seen();
    if before != seen {
        return Err(SourceError::Changing);
    }
    // An earlier gap error must clear even if the file returns unchanged.
    if snapshot.take().is_some_and(|previous| previous == seen) && !error_reported {
        return Ok(None);
    }
    parse(path, &after, previous, scope).map(Some)
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
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Avoid implicitly recalling remote-storage data if the provider state
        // changes between the path check and open. Check handle metadata too.
        options.custom_flags(0x0010_0000); // FILE_FLAG_OPEN_NO_RECALL
    }
    options.open(path)
}
#[allow(clippy::disallowed_methods)]
fn read_open_file(file: std::fs::File) -> Result<String, SourceError> {
    let metadata = file.metadata().map_err(|_| SourceError::Unavailable)?;
    if !metadata.is_file() {
        return Err(SourceError::NotRegular);
    }
    if metadata.len() > MAX_LIBRARY_BYTES {
        return Err(SourceError::TooLarge);
    }
    io::read_if_downloaded(io::needs_download(&metadata), || read_text(file, metadata.len()))
}

#[cfg(test)]
#[cfg(unix)]
#[path = "../tests/support/subprocess_watchdog.rs"]
mod subprocess_watchdog;

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn attachment_files_stay_inside_the_library_folder_unless_allowed() {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path().join("library");
        std::fs::create_dir_all(base.join("papers")).unwrap();
        std::fs::write(base.join("papers/atlas.pdf"), b"%PDF-1.7\n").unwrap();
        let outside = directory.path().join("outside.pdf");
        std::fs::write(&outside, b"%PDF-1.7\n").unwrap();
        let library = base.join("refs.bib");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, base.join("escape.pdf")).unwrap();
        let locations = [
            ("papers/atlas.pdf".into(), false),
            (
                base.join("papers/atlas.pdf").to_string_lossy().replace('\\', "/"),
                false,
            ),
            ("../outside.pdf".into(), true),
            (outside.to_string_lossy().replace('\\', "/"), true),
            #[cfg(unix)]
            ("escape.pdf".into(), true),
        ];
        for (location, escapes) in locations {
            std::fs::write(
                &library,
                format!("@book{{atlas, title={{Invented atlas}}, file={{{location}}}}}"),
            )
            .unwrap();
            let loaded = reload_file(&library).unwrap();
            let parent = loaded
                .snapshot
                .objects()
                .iter()
                .find(|object| matches!(object.object, LibraryObject::Record(_)))
                .unwrap();
            let key = loaded
                .snapshot
                .objects()
                .iter()
                .find(|object| matches!(object.object, LibraryObject::Attachment(_)))
                .unwrap()
                .object
                .id()
                .key
                .clone();
            let find = |allowed| {
                attachment_file(
                    &library,
                    parent.object.id(),
                    &parent.fingerprints.full,
                    &key,
                    allowed,
                    &|| false,
                )
            };
            if escapes {
                assert_eq!(find(false), Err(SourceError::AttachmentOutsideLibrary));
                assert!(find(true).unwrap().is_some());
            } else {
                assert!(find(false).unwrap().is_some());
            }
        }
    }
    #[test]
    fn manual_sources_report_live_refresh_off_without_starting_a_worker() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("synthetic-network.bib");
        std::fs::write(&path, "@misc{before}").unwrap_or_else(|e| panic!("{e}"));
        let (send, events) = mpsc::channel();
        let result = SourceWatch::start_with_policy(
            &path,
            None,
            None,
            move |candidate| {
                let _ = send.send(candidate);
                None
            },
            |_| false,
        ); // Event-less filesystem, regardless of native registration success.
        assert_eq!(result.err(), Some(SourceError::WatchUnavailable));
        assert!(
            matches!(events.try_recv(), Err(mpsc::TryRecvError::Disconnected)),
            "manual sources do no background work"
        );
        std::fs::write(&path, "@misc{after}").unwrap_or_else(|e| panic!("{e}"));
        let refreshed = reload_file(&path).unwrap_or_else(|e| panic!("{e}"));
        let LibraryObject::Record(record) = &refreshed.snapshot.objects()[0].object else {
            panic!("reference")
        };
        assert_eq!(record.citation_key(), Ok(Some("after")));
    }

    #[cfg(unix)]
    #[test]
    fn completed_file_observation_refuses_a_symlink_retarget_during_read() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let selected = dir.path().join("selected.bib");
        let a = dir.path().join("a.bib");
        let b = dir.path().join("b.bib");
        std::fs::write(&a, "@book{a}").unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(&b, "@book{b}").unwrap_or_else(|e| panic!("{e}"));
        std::os::unix::fs::symlink(&a, &selected).unwrap_or_else(|e| panic!("{e}"));
        let result = load_file_snapshot_with(
            &selected,
            || {
                std::fs::remove_file(&selected).unwrap_or_else(|e| panic!("{e}"));
                std::os::unix::fs::symlink(&b, &selected).unwrap_or_else(|e| panic!("{e}"));
            },
            &io::Scope::new(&|| false),
        );
        assert!(
            matches!(result, Err(SourceError::Changing)),
            "retargeted bytes must never acquire the other source identity"
        );
    }

    #[test]
    fn a_reload_refuses_bytes_that_differ_between_its_two_reads() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = dir.path().join("synthetic.bib");
        std::fs::write(&path, "@misc{done}\n@misc{half, title={Half").unwrap_or_else(|e| panic!("{e}"));
        // A save still being written: the second read sees different bytes.
        let result = stable_observation_with(
            &path,
            || {
                std::fs::write(&path, "@misc{done}\n@misc{half, title={Half written}}")
                    .unwrap_or_else(|e| panic!("{e}"));
            },
            &io::Scope::new(&|| false),
        );
        assert!(matches!(result, Err(SourceError::Changing)));
        // Once it settles, the finished save loads whole.
        let settled = reload_file(&path).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(settled.snapshot.objects().len(), 2);
        assert!(settled.warnings.is_empty());
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
}
