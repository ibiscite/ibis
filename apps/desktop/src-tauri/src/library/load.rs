//! Reading a library file or Zotero into a new library, off the main thread.

use std::path::PathBuf;

use super::error::LoadError;
use super::source::Source;
use super::state::Library;

/// Read a library file through the engine off the main thread. Initial loads
/// tolerate damaged entries; rereads keep the last good references when the
/// file is demonstrably partial.
#[cfg(test)]
pub(crate) fn load_library(path: PathBuf, reload: bool) -> Result<Library, LoadError> {
    match load_file(path, reload, None, &ibis_engine::Cancellation::default(), &|| false)? {
        Loaded::New(library) => Ok(*library),
        Loaded::Unchanged | Loaded::Reload(_) => Err(LoadError::Stopped),
    }
}

fn load_file(
    path: PathBuf,
    reload: bool,
    previous: Option<std::sync::Arc<super::engine_file::FileLibrary>>,
    cancel: &ibis_engine::Cancellation,
    cancelled: &impl Fn() -> bool,
) -> Result<Loaded, LoadError> {
    check_cancelled(cancelled)?;
    let (file, unchanged) = match previous {
        Some(file) => {
            let before = file.revision();
            if reload {
                let prepared = file.prepare_reload(&path, cancel)?;
                check_cancelled(cancelled)?;
                return Ok(Loaded::Reload(Box::new(LibraryReload {
                    source: Source::File(path),
                    file,
                    prepared: PreparedReload::File(prepared),
                })));
            }
            // An unreadable snapshot has no revision and never counts as unchanged.
            let unchanged = !before.is_empty() && before == file.revision();
            (file, unchanged)
        }
        None => (
            std::sync::Arc::new(super::engine_file::FileLibrary::open(&path)?),
            false,
        ),
    };
    check_cancelled(cancelled)?;
    if unchanged {
        return Ok(Loaded::Unchanged);
    }
    Ok(Loaded::New(Box::new(Library {
        source: Some(Source::File(path)),
        file: Some(file),
        checked_at: Some(std::time::SystemTime::now()),
        ..Library::default()
    })))
}

fn check_cancelled(cancelled: &impl Fn() -> bool) -> Result<(), LoadError> {
    if cancelled() { Err(LoadError::Stopped) } else { Ok(()) }
}

/// What a background load produced.
pub(crate) enum Loaded {
    New(Box<Library>),
    /// A reread found nothing changed; keep the current references.
    Unchanged,
    /// A strict read outcome, including failure, still private until acceptance.
    Reload(Box<LibraryReload>),
}

pub(super) struct LibraryReload {
    pub(super) source: Source,
    pub(super) file: std::sync::Arc<super::engine_file::FileLibrary>,
    pub(super) prepared: PreparedReload,
}

pub(super) enum PreparedReload {
    File(ibis_engine::PreparedFileReload),
    Zotero(ibis_engine::PreparedZoteroReload),
}

impl LibraryReload {
    pub(super) fn accept(self) -> Result<(), LoadError> {
        match self.prepared {
            PreparedReload::File(prepared) => self.file.accept_reload(prepared),
            PreparedReload::Zotero(prepared) => self
                .file
                .engine
                .accept_zotero_reload(prepared)
                .map_err(LoadError::Zotero),
        }
    }
}

#[cfg(test)]
pub(super) fn load_source(
    source: Source,
    reload: bool,
    previous_file: Option<std::sync::Arc<super::engine_file::FileLibrary>>,
    profile: Option<&str>,
    cancel: &ibis_engine::Cancellation,
    cancelled: impl Fn() -> bool,
) -> Result<Loaded, LoadError> {
    load_source_guarded(source, reload, previous_file, profile, cancel, cancelled, |retire| {
        retire()
    })
}

pub(super) fn load_source_guarded(
    source: Source,
    reload: bool,
    previous_file: Option<std::sync::Arc<super::engine_file::FileLibrary>>,
    profile: Option<&str>,
    cancel: &ibis_engine::Cancellation,
    cancelled: impl Fn() -> bool,
    guard: impl FnMut(
        &mut dyn FnMut() -> Result<(), ibis_core::contract::EngineError>,
    ) -> Result<(), ibis_core::contract::EngineError>,
) -> Result<Loaded, LoadError> {
    check_cancelled(&cancelled)?;
    if cancel.is_cancelled() {
        return Err(LoadError::Stopped);
    }
    match source {
        Source::File(path) => load_file(path, reload, previous_file, cancel, &cancelled),
        Source::Samples(path) => {
            // A separate engine, using the normal CSL-JSON file reader. The
            // bundled immutable file needs neither a watch nor a saved choice.
            let file = super::engine_file::FileLibrary::open_samples(&path)?;
            check_cancelled(&cancelled)?;
            Ok(Loaded::New(Box::new(Library {
                source: Some(Source::Samples(path)),
                file: Some(std::sync::Arc::new(file)),
                checked_at: Some(std::time::SystemTime::now()),
                ..Library::default()
            })))
        }
        Source::Zotero(chosen) => {
            if let Some(bridge) = previous_file {
                let prepared = bridge.prepare_zotero_reload(&chosen, profile, reload, cancel, guard)?;
                check_cancelled(&cancelled)?;
                cancel.check().map_err(LoadError::Zotero)?;
                return Ok(Loaded::Reload(Box::new(LibraryReload {
                    source: Source::Zotero(chosen),
                    file: bridge,
                    prepared: PreparedReload::Zotero(prepared),
                })));
            }
            let bridge = std::sync::Arc::new(super::engine_file::FileLibrary::open_zotero(&chosen, profile, cancel)?);
            check_cancelled(&cancelled)?;
            if cancel.is_cancelled() {
                return Err(LoadError::Stopped);
            }
            Ok(Loaded::New(Box::new(Library {
                source: Some(Source::Zotero(chosen)),
                file: Some(bridge),
                checked_at: Some(std::time::SystemTime::now()),
                ..Library::default()
            })))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::settings::ZoteroLibrary;

    #[test]
    fn cancelled_loads_stop_before_source_acquisition() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = dir.path().join("synthetic.bib");
        for source in [Source::File(file.clone()), Source::Zotero(ZoteroLibrary::User.into())] {
            for cancel_engine in [false, true] {
                let cancel = ibis_engine::Cancellation::default();
                if cancel_engine {
                    cancel.cancel();
                }
                assert!(matches!(
                    load_source(source.clone(), false, None, None, &cancel, || !cancel_engine),
                    Err(LoadError::Stopped)
                ));
            }
        }
        std::fs::write(&file, "@misc{synthetic}").unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(
            load_file(file, false, None, &ibis_engine::Cancellation::default(), &|| true),
            Err(LoadError::Stopped)
        ));
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
        assert_eq!(
            status
                .reading
                .iter()
                .map(|notice| notice.code.as_str())
                .collect::<Vec<_>>(),
            ["syntax"]
        );
        let file = library.file.as_ref().unwrap_or_else(|| panic!("engine file"));
        let query = serde_json::from_value(serde_json::json!({"text":"sleep","interpret_locators":false}))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            file.search(&query, 10, None, &ibis_engine::Cancellation::default())
                .unwrap_or_else(|e| panic!("{e}"))
                .results
                .len(),
            1
        );
        assert!(load_library(std::env::temp_dir().join("ibis-missing-library.bib"), false).is_err());
    }
}
