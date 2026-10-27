//! Reading a library file or Zotero into a new library, off the main thread.

use std::path::PathBuf;

use ibis_core::settings::ZoteroLibraries;
use ibis_core::zotero::api::{Response, Transport, TransportError};
use ibis_core::zotero::{self, libraries};
use ibis_zotero_local::LocalZotero;

use super::error::LoadError;
use super::source::{Source, zotero_library};
use super::state::{Library, candidate};

/// Read, parse and index off the main thread. Initial loads tolerate damaged
/// entries; reloads retain the last good snapshot for demonstrably partial data.
#[cfg(test)]
pub(crate) fn load_library(path: PathBuf, reload: bool) -> Result<Library, LoadError> {
    load_file(path, reload, None, &|| false)
}

fn load_file(
    path: PathBuf,
    reload: bool,
    previous: Option<std::sync::Arc<super::engine_file::FileLibrary>>,
    cancelled: &impl Fn() -> bool,
) -> Result<Library, LoadError> {
    check_cancelled(cancelled)?;
    let file = match previous {
        Some(file) => file,
        None => std::sync::Arc::new(super::engine_file::FileLibrary::open(&path)?),
    };
    if reload {
        file.reload()?;
    }
    check_cancelled(cancelled)?;
    Ok(Library {
        source: Some(Source::File(path)),
        file: Some(file),
        checked_at: Some(std::time::SystemTime::now()),
        ..Library::default()
    })
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
/// holds the versions already loaded.
pub(super) fn load_zotero(
    chosen: ZoteroLibraries,
    previous: Option<&libraries::Snapshots>,
    zotero: &mut impl Transport,
    cancelled: impl Fn() -> bool,
) -> Result<Loaded, LoadError> {
    let mut zotero = Cancellable {
        transport: zotero,
        cancelled: &cancelled,
    };
    let wanted: Vec<zotero::Library> = chosen.libraries().iter().copied().map(zotero_library).collect();
    let refreshed = libraries::refresh(&mut zotero, &wanted, previous);
    check_cancelled(&cancelled)?;
    match refreshed? {
        libraries::Refresh::Unchanged => Ok(Loaded::Unchanged),
        libraries::Refresh::Loaded { result, snapshots } => {
            let mut new = candidate(Source::Zotero(chosen), result);
            new.zotero = Some(snapshots);
            Ok(Loaded::New(Box::new(new)))
        }
    }
}

pub(super) fn load_source(
    source: Source,
    reload: bool,
    previous: Option<libraries::Snapshots>,
    previous_file: Option<std::sync::Arc<super::engine_file::FileLibrary>>,
    cancelled: impl Fn() -> bool,
) -> Result<Loaded, LoadError> {
    check_cancelled(&cancelled)?;
    match source {
        Source::File(path) => {
            load_file(path, reload, previous_file, &cancelled).map(|library| Loaded::New(Box::new(library)))
        }
        Source::Zotero(chosen) => load_zotero(
            chosen,
            previous.as_ref().filter(|_| reload),
            &mut LocalZotero::default(),
            cancelled,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::settings::ZoteroLibrary;
    use std::cell::Cell;

    #[test]
    fn cancellation_stops_zotero_pages_and_later_libraries_before_indexing() {
        struct Pages<'a> {
            cancelled: &'a Cell<bool>,
            reads: usize,
            total: usize,
        }
        impl Transport for Pages<'_> {
            fn get(&mut self, _: &str) -> Result<Response, TransportError> {
                self.reads += 1;
                self.cancelled.set(true);
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
        for total in [0, 1, 2] {
            let cancelled = Cell::new(false);
            let mut pages = Pages {
                cancelled: &cancelled,
                reads: 0,
                total,
            };
            let loaded = load_zotero(chosen.clone(), None, &mut pages, || cancelled.get());
            assert_eq!(pages.reads, 1, "no second page or later library may be read");
            assert!(matches!(loaded, Err(LoadError::Stopped)), "obsolete load must stop");
        }
    }

    #[test]
    fn cancelled_file_loads_stop_before_io_or_indexing() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = dir.path().join("synthetic.bib");
        assert!(matches!(
            load_source(Source::File(file.clone()), false, None, None, || true),
            Err(LoadError::Stopped)
        ));
        std::fs::write(&file, "@misc{synthetic}").unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(
            load_file(file, false, None, &|| true),
            Err(LoadError::Stopped)
        ));
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
        let status = super::super::status::status(&library);
        assert_eq!(status.references, 1);
        assert_eq!(status.warnings, 1);
        let file = library.file.as_ref().unwrap_or_else(|| panic!("engine file"));
        let query = serde_json::from_value(serde_json::json!({"text":"sleep","interpret_locators":false}))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            file.search(&query, 10, None)
                .unwrap_or_else(|e| panic!("{e}"))
                .results
                .len(),
            1
        );
        assert!(load_library(std::env::temp_dir().join("ibis-missing-library.bib"), false).is_err());
    }
}
