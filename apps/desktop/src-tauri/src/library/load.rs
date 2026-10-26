//! Reading a library file or Zotero into a new library, off the main thread.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ibis_core::model::{LoadResult, LoadWarning, WarningKind};
use ibis_core::settings::ZoteroLibraries;
use ibis_core::source::load_library_snapshot;
use ibis_core::zotero::api::{self, Response, Transport, TransportError, ZoteroError};
use ibis_core::zotero::{self, libraries};
use ibis_zotero_local::LocalZotero;
use ibis_zotero_local::discovery::{DiscoveryRoots, Platform, discover_until};
use ibis_zotero_local::snapshot::{self, Fingerprint};

use super::error::LoadError;
use super::source::{Source, zotero_library};
use super::state::{Library, candidate};

/// Read, parse and index off the main thread. Initial loads tolerate damaged
/// entries; reloads retain the last good snapshot for demonstrably partial data.
#[cfg(test)]
pub(crate) fn load_library(path: PathBuf, reload: bool) -> Result<Library, LoadError> {
    load_file(path, reload, &|| false)
}

fn load_file(path: PathBuf, reload: bool, cancelled: &impl Fn() -> bool) -> Result<Library, LoadError> {
    let (loaded, snapshot) = load_library_snapshot(&path, reload)?;
    check_cancelled(cancelled)?;
    let mut library = candidate(path, loaded);
    library.snapshot = Some(snapshot);
    Ok(library)
}

fn check_cancelled(cancelled: &impl Fn() -> bool) -> Result<(), LoadError> {
    if cancelled() { Err(LoadError::Stopped) } else { Ok(()) }
}

struct Cancellable<'a, T, C> {
    transport: &'a mut T,
    cancelled: C,
}

impl<T: Transport, C: Fn() -> bool> Transport for Cancellable<'_, T, C> {
    fn get(&mut self, path: &str) -> Result<Response, TransportError> {
        // Unreachable aborts the whole multi-library scan. The obsolete
        // request is discarded, so this sentinel never reaches the interface.
        if (self.cancelled)() {
            return Err(TransportError::Unreachable);
        }
        let response = self.transport.get(path);
        if (self.cancelled)() {
            return Err(TransportError::Unreachable);
        }
        response
    }
}

/// What a background load produced.
pub(crate) enum Loaded {
    New(Box<Library>),
    /// A Zotero refresh found nothing changed; keep the current references.
    Unchanged,
}

/// Read the Zotero libraries, or only check for changes when `previous`
/// holds the versions already loaded. When Zotero is not running, a copy of
/// its database is read instead (ADR 0027).
pub(super) fn load_zotero(
    chosen: ZoteroLibraries,
    previous: Option<&libraries::Snapshots>,
    reload: bool,
    zotero: &mut impl Transport,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<Loaded, LoadError> {
    let mut zotero = Cancellable {
        transport: zotero,
        cancelled: || cancelled(),
    };
    let wanted: Vec<zotero::Library> = chosen.libraries().iter().copied().map(zotero_library).collect();
    let refreshed = libraries::refresh(&mut zotero, &wanted, previous);
    check_cancelled(&|| cancelled())?;
    match refreshed {
        Err(ZoteroError::Transport(TransportError::Unreachable)) => {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
            let stopped: Arc<dyn Fn() -> bool + Send + Sync> =
                Arc::new(move || cancelled() || std::time::Instant::now() >= deadline);
            let data = zotero_data(&|| stopped());
            check_cancelled(&|| stopped())?;
            load_closed(chosen, &wanted, reload && previous.is_none(), data, stopped)
        }
        outcome => installed(chosen, outcome?, &|| cancelled()),
    }
}

fn installed(
    chosen: ZoteroLibraries,
    refresh: libraries::Refresh,
    stop: &dyn Fn() -> bool,
) -> Result<Loaded, LoadError> {
    match refresh {
        libraries::Refresh::Unchanged => Ok(Loaded::Unchanged),
        libraries::Refresh::Loaded { result, snapshots } => {
            let mut new = indexed_zotero(chosen, result, stop)?;
            new.zotero = Some(snapshots);
            Ok(Loaded::New(Box::new(new)))
        }
    }
}

fn indexed_zotero(chosen: ZoteroLibraries, result: LoadResult, stop: &dyn Fn() -> bool) -> Result<Library, LoadError> {
    let index = ibis_core::search::OwnedIndex::new_until(result.references, stop).ok_or(LoadError::Stopped)?;
    Ok(Library {
        source: Some(Source::Zotero(chosen)),
        checked_at: Some(std::time::SystemTime::now()),
        index: Arc::new(index),
        warnings: result.warnings,
        ..Library::default()
    })
}

fn refresh_running(
    chosen: ZoteroLibraries,
    wanted: &[zotero::Library],
    transport: &mut impl Transport,
    cancelled: &Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<Loaded, LoadError> {
    let mut transport = Cancellable {
        transport,
        cancelled: || cancelled(),
    };
    let refresh = libraries::refresh(&mut transport, wanted, None);
    check_cancelled(&|| cancelled())?;
    installed(chosen, refresh?, &|| cancelled())
}

/// The last libraries read while Zotero was closed, with the state of the
/// database files they came from, so showing the picker again can skip
/// converting unchanged content after a coordinated copy. A hit returns the saved result, never "unchanged",
/// so whatever the window holds is replaced by what the files say.
type Closed = (ZoteroLibraries, PathBuf, Fingerprint, LoadResult);
static LAST_CLOSED: Mutex<Option<Closed>> = Mutex::new(None);

/// Read Zotero's libraries from a copy of its database. Any failure reads
/// as "Zotero is closed", whose remedy (open Zotero) always works.
fn load_closed(
    chosen: ZoteroLibraries,
    wanted: &[zotero::Library],
    reload: bool,
    data: Option<PathBuf>,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<Loaded, LoadError> {
    let closed = LoadError::Zotero(ZoteroError::Transport(TransportError::Unreachable));
    let started = std::time::Instant::now();
    let data = data.ok_or(closed)?;
    // The lock is held only to look up and to store, never while reading.
    let saved = LAST_CLOSED
        .lock()
        .ok()
        .and_then(|last| last.clone())
        .filter(|(seen, folder, ..)| reload && seen == &chosen && folder == &data);
    let scratch = scratch().ok_or(closed)?;
    check_cancelled(&|| cancelled())?;
    let outcome = snapshot::read_cancellable(&data, &scratch, wanted, saved.as_ref().map(|s| &s.2), cancelled.clone());
    check_cancelled(&|| cancelled())?;
    // Zotero may have started meanwhile; then only its own answer counts.
    if api::presence(&mut LocalZotero::default()) != api::Presence::Absent {
        return refresh_running(chosen, wanted, &mut LocalZotero::default(), &cancelled);
    }
    let result = match outcome {
        Ok(snapshot::Refresh::Unchanged) => saved.ok_or(closed)?.3,
        Ok(snapshot::Refresh::Loaded {
            mut result,
            fingerprint,
        }) => {
            if snapshot::cloud_synced(&data) {
                result.warnings.push(LoadWarning {
                    kind: WarningKind::Field,
                    line: None,
                    key: None,
                    message: CLOUD_SYNCED.to_string(),
                });
            }
            if let Ok(mut last) = LAST_CLOSED.lock() {
                *last = Some((chosen.clone(), data, fingerprint, result.clone()));
            }
            result
        }
        Err(error) => {
            eprintln!("ibis-zotero-closed: {error}");
            return Err(if error == snapshot::SnapshotError::UnsupportedKeys {
                LoadError::ZoteroClosedKeys
            } else {
                closed
            });
        }
    };
    check_cancelled(&|| cancelled())?;
    let stop = || cancelled() || started.elapsed() >= std::time::Duration::from_secs(60);
    Ok(Loaded::New(Box::new(indexed_zotero(chosen, result, &stop)?)))
}

/// Where the copy is made. On Windows `TMP` may point at a folder other
/// users can read, so the copy goes in the profile's temporary folder and
/// receives its own protected owner-only ACL before any contents are copied.
fn scratch() -> Option<PathBuf> {
    if cfg!(windows) {
        let local = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Temp");
        (local.is_absolute() && snapshot::local(&local) && local.is_dir()).then_some(local)
    } else {
        Some(std::env::temp_dir())
    }
}

const CLOUD_SYNCED: &str = "Zotero's data folder is inside a folder a cloud service syncs. Zotero advises \
     against this: syncing can damage its database. Ibis changes nothing; move the folder in Zotero's \
     settings to keep it safe.";

/// Zotero's data folder on this computer, when exactly one is found and it
/// holds a database. Several are a choice for the user, never a guess.
fn zotero_data(stop: &dyn Fn() -> bool) -> Option<PathBuf> {
    let platform = if cfg!(target_os = "macos") {
        Platform::MacOs
    } else if cfg!(windows) {
        Platform::Windows
    } else {
        Platform::Linux
    };
    let roaming = std::env::var_os("APPDATA").map(PathBuf::from);
    let found = discover_until(
        &DiscoveryRoots::new(platform, std::env::home_dir()?, roaming),
        None,
        stop,
    )?;
    let mut folders = found
        .directories
        .into_iter()
        .filter(|d| !stop() && snapshot::local(d.path()));
    match (folders.next(), folders.next()) {
        (Some(one), None) => Some(one.path().to_path_buf()),
        _ => None,
    }
}

pub(super) fn load_source(
    source: Source,
    reload: bool,
    previous: Option<libraries::Snapshots>,
    cancelled: impl Fn() -> bool + Send + Sync + 'static,
) -> Result<Loaded, LoadError> {
    check_cancelled(&cancelled)?;
    match source {
        Source::File(path) => load_file(path, reload, &cancelled).map(|library| Loaded::New(Box::new(library))),
        Source::Zotero(chosen) => load_zotero(
            chosen,
            previous.as_ref().filter(|_| reload),
            reload,
            &mut LocalZotero::default(),
            Arc::new(cancelled),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::settings::ZoteroLibrary;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn cancellation_stops_zotero_pages_and_later_libraries_before_indexing() {
        struct Pages {
            cancelled: Arc<AtomicBool>,
            reads: usize,
            total: usize,
        }
        impl Transport for Pages {
            fn get(&mut self, _: &str) -> Result<Response, TransportError> {
                self.reads += 1;
                self.cancelled.store(true, Ordering::SeqCst);
                Ok(Response {
                    status: 200,
                    headers: vec![
                        ("Zotero-API-Version".into(), "3".into()),
                        ("Last-Modified-Version".into(), "1".into()),
                        ("Total-Results".into(), self.total.to_string()),
                    ],
                    body: if self.total == 0 {
                        b"[]".to_vec()
                    } else {
                        br#"[{"key":"SYN00001","data":{"itemType":"book"},"csljson":{"type":"book","title":"Synthetic"}}]"#.to_vec()
                    },
                })
            }
        }
        let chosen = ZoteroLibraries::new(vec![ZoteroLibrary::User, ZoteroLibrary::Group(7)])
            .unwrap_or_else(|| panic!("valid libraries"));
        for post_snapshot in [false, true] {
            for total in [0, 1, 2] {
                let cancelled = Arc::new(AtomicBool::new(false));
                let mut pages = Pages {
                    cancelled: cancelled.clone(),
                    reads: 0,
                    total,
                };
                let stop: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || cancelled.load(Ordering::SeqCst));
                let loaded = if post_snapshot {
                    refresh_running(
                        chosen.clone(),
                        &[zotero::Library::User, zotero::Library::Group(7)],
                        &mut pages,
                        &stop,
                    )
                } else {
                    load_zotero(chosen.clone(), None, false, &mut pages, stop)
                };
                assert_eq!(pages.reads, 1, "no second page or later library may be read");
                assert!(matches!(loaded, Err(LoadError::Stopped)), "obsolete load must stop");
            }
        }
    }

    #[test]
    fn cancelled_file_loads_stop_before_io_or_indexing() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = dir.path().join("synthetic.bib");
        assert!(matches!(
            load_source(Source::File(file.clone()), false, None, || true),
            Err(LoadError::Stopped)
        ));
        std::fs::write(&file, "@misc{synthetic}").unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(load_file(file, false, &|| true), Err(LoadError::Stopped)));
    }

    #[test]
    fn only_regular_files_are_read() {
        assert!(ibis_core::source::read_library_file(std::path::Path::new(".")).is_err());
        #[cfg(unix)]
        assert!(ibis_core::source::read_library_file(std::path::Path::new("/dev/zero")).is_err());
        let file = std::env::temp_dir().join(format!("ibis-read-{}.bib", std::process::id()));
        std::fs::write(&file, "@misc{k}").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ibis_core::source::read_library_file(&file).as_deref(), Ok("@misc{k}"));
    }

    #[test]
    fn a_loaded_library_is_indexed_once_and_searchable() {
        let file = std::env::temp_dir().join(format!("ibis-load-{}.bib", std::process::id()));
        std::fs::write(&file, "@article{smith2020, title={Sleep and memory}}\n@article{bad,\n")
            .unwrap_or_else(|e| panic!("{e}"));
        let library = load_library(file, false).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(library.index.references().len(), 1);
        assert_eq!(library.warnings.len(), 1);
        assert!(library.snapshot.is_some());
        assert_eq!(library.index.search("sleep", 10).len(), 1);
        assert!(load_library(std::env::temp_dir().join("ibis-missing-library.bib"), false).is_err());
    }

    #[test]
    fn without_a_zotero_data_folder_a_closed_zotero_stays_closed() {
        let chosen = ZoteroLibraries::from(ibis_core::settings::ZoteroLibrary::User);
        let missing = std::env::temp_dir().join(format!("ibis-no-zotero-{}", std::process::id()));
        for data in [None, Some(missing)] {
            assert_eq!(
                load_closed(
                    chosen.clone(),
                    &[zotero::Library::User],
                    false,
                    data,
                    Arc::new(|| false)
                )
                .err(),
                Some(super::super::error::ZOTERO_OFFLINE)
            );
        }
    }
}
