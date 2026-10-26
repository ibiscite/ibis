//! Reading a library file or Zotero into a new library, off the main thread.

use std::path::PathBuf;
use std::sync::Mutex;

use ibis_core::model::{LoadResult, LoadWarning, WarningKind};
use ibis_core::settings::ZoteroLibraries;
use ibis_core::source::load_library_snapshot;
use ibis_core::zotero::api::{self, TransportError, ZoteroError};
use ibis_core::zotero::{self, libraries};
use ibis_zotero_local::LocalZotero;
use ibis_zotero_local::discovery::{DiscoveryRoots, Platform, discover};
use ibis_zotero_local::snapshot::{self, Fingerprint};

use super::error::LoadError;
use super::source::{Source, zotero_library};
use super::state::{Library, candidate};

/// Read, parse and index off the main thread. Initial loads tolerate damaged
/// entries; reloads retain the last good snapshot for demonstrably partial data.
pub(crate) fn load_library(path: PathBuf, reload: bool) -> Result<Library, LoadError> {
    let (loaded, snapshot) = load_library_snapshot(&path, reload)?;
    let mut library = candidate(path, loaded);
    library.snapshot = Some(snapshot);
    Ok(library)
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
) -> Result<Loaded, LoadError> {
    let mut zotero = LocalZotero::default();
    let wanted: Vec<zotero::Library> = chosen.libraries().iter().copied().map(zotero_library).collect();
    match libraries::refresh(&mut zotero, &wanted, previous) {
        Err(ZoteroError::Transport(TransportError::Unreachable)) => {
            load_closed(chosen, &wanted, reload && previous.is_none(), zotero_data())
        }
        outcome => installed(chosen, outcome?),
    }
}

fn installed(chosen: ZoteroLibraries, refresh: libraries::Refresh) -> Result<Loaded, LoadError> {
    match refresh {
        libraries::Refresh::Unchanged => Ok(Loaded::Unchanged),
        libraries::Refresh::Loaded { result, snapshots } => {
            let mut new = candidate(Source::Zotero(chosen), result);
            new.zotero = Some(snapshots);
            Ok(Loaded::New(Box::new(new)))
        }
    }
}

/// The last libraries read while Zotero was closed, with the state of the
/// database files they came from, so showing the picker again does not copy
/// an unchanged database. A hit returns the saved result, never "unchanged",
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
) -> Result<Loaded, LoadError> {
    let closed = LoadError::Zotero(ZoteroError::Transport(TransportError::Unreachable));
    let data = data.ok_or(closed)?;
    // The lock is held only to look up and to store, never while reading.
    let saved = LAST_CLOSED
        .lock()
        .ok()
        .and_then(|last| last.clone())
        .filter(|(seen, folder, ..)| reload && seen == &chosen && folder == &data);
    let outcome = snapshot::read(&data, &std::env::temp_dir(), wanted, saved.as_ref().map(|s| &s.2));
    // Zotero may have started meanwhile; then only its own answer counts.
    if api::presence(&mut LocalZotero::default()) != api::Presence::Absent {
        return installed(
            chosen.clone(),
            libraries::refresh(&mut LocalZotero::default(), wanted, None)?,
        );
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
            return Err(closed);
        }
    };
    Ok(Loaded::New(Box::new(candidate(Source::Zotero(chosen), result))))
}

const CLOUD_SYNCED: &str = "Zotero's data folder is inside a folder a cloud service syncs. Zotero advises \
     against this: syncing can damage its database. Ibis changes nothing; move the folder in Zotero's \
     settings to keep it safe.";

/// Zotero's data folder on this computer, when exactly one is found and it
/// holds a database. Several are a choice for the user, never a guess.
fn zotero_data() -> Option<PathBuf> {
    let platform = if cfg!(target_os = "macos") {
        Platform::MacOs
    } else if cfg!(windows) {
        Platform::Windows
    } else {
        Platform::Linux
    };
    let roaming = std::env::var_os("APPDATA").map(PathBuf::from);
    let found = discover(&DiscoveryRoots::new(platform, std::env::home_dir()?, roaming), None);
    let mut folders = found
        .directories
        .into_iter()
        .filter(|d| snapshot::local(d.path()) && d.path().join("zotero.sqlite").is_file());
    match (folders.next(), folders.next()) {
        (Some(one), None) => Some(one.path().to_path_buf()),
        _ => None,
    }
}

pub(super) fn load_source(
    source: Source,
    reload: bool,
    previous: Option<libraries::Snapshots>,
) -> Result<Loaded, LoadError> {
    match source {
        Source::File(path) => load_library(path, reload).map(|library| Loaded::New(Box::new(library))),
        Source::Zotero(chosen) => load_zotero(chosen, previous.as_ref().filter(|_| reload), reload),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                load_closed(chosen.clone(), &[zotero::Library::User], false, data).err(),
                Some(super::super::error::ZOTERO_OFFLINE)
            );
        }
    }
}
