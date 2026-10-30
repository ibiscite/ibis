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
    match load_file(path, reload, None, &|| false)? {
        Loaded::New(library) => Ok(*library),
        Loaded::Unchanged => Err(LoadError::Stopped),
    }
}

fn load_file(
    path: PathBuf,
    reload: bool,
    previous: Option<std::sync::Arc<super::engine_file::FileLibrary>>,
    cancelled: &impl Fn() -> bool,
) -> Result<Loaded, LoadError> {
    check_cancelled(cancelled)?;
    let (file, unchanged) = match previous {
        Some(file) => {
            let before = file.revision();
            if reload {
                file.reload()?;
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
}

pub(super) fn load_source_cached(
    source: Source,
    reload: bool,
    previous: Option<std::sync::Arc<super::engine_file::FileLibrary>>,
    cancel: &ibis_engine::Cancellation,
    cache: ibis_engine::FileCacheOperation,
    cancelled: impl Fn() -> bool,
) -> Result<Loaded, LoadError> {
    let Source::File(path) = source else {
        return load_source(source, reload, previous, cancel, cancelled);
    };
    check_cancelled(&cancelled)?;
    let file = match previous {
        Some(file) => file,
        None => std::sync::Arc::new(super::engine_file::FileLibrary::from_engine(
            ibis_engine::Engine::new().map_err(|_| LoadError::Stopped)?,
        )),
    };
    let pending = file
        .engine
        .prepare_file_load(&path, reload, cache, cancel)
        .map_err(|error| super::engine_file::file_load_error(error, false))?;
    check_cancelled(&cancelled)?;
    let file = pending
        .engine()
        .map(|engine| std::sync::Arc::new(super::engine_file::FileLibrary::from_engine(engine)))
        .unwrap_or(file);
    let cached = pending.cache_hit();
    Ok(Loaded::New(Box::new(Library {
        source: Some(Source::File(path.clone())),
        requested: cached.then_some(Source::File(path)),
        opening: cached,
        file: Some(file),
        pending_file: Some(pending),
        checked_at: (!cached).then(std::time::SystemTime::now),
        ..Library::default()
    })))
}

pub(super) fn load_source(
    source: Source,
    reload: bool,
    previous_file: Option<std::sync::Arc<super::engine_file::FileLibrary>>,
    cancel: &ibis_engine::Cancellation,
    cancelled: impl Fn() -> bool,
) -> Result<Loaded, LoadError> {
    check_cancelled(&cancelled)?;
    if cancel.is_cancelled() {
        return Err(LoadError::Stopped);
    }
    match source {
        Source::File(path) => load_file(path, reload, previous_file, &cancelled),
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
            let (bridge, unchanged) = if let Some(bridge) = previous_file {
                let before = bridge.revision();
                let same_choice = bridge.refresh_zotero(&chosen, reload, cancel)?;
                let unchanged = same_choice && before == bridge.revision();
                (bridge, unchanged)
            } else {
                (
                    std::sync::Arc::new(super::engine_file::FileLibrary::open_zotero(&chosen, cancel)?),
                    false,
                )
            };
            check_cancelled(&cancelled)?;
            if cancel.is_cancelled() {
                return Err(LoadError::Stopped);
            }
            if unchanged {
                return Ok(Loaded::Unchanged);
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
                    load_source(source.clone(), false, None, &cancel, || !cancel_engine),
                    Err(LoadError::Stopped)
                ));
            }
        }
        std::fs::write(&file, "@misc{synthetic}").unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(
            load_file(file, false, None, &|| true),
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
