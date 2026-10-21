//! Reading a library file or Zotero into a new library, off the main thread.

use std::path::PathBuf;

use ibis_core::settings::ZoteroLibraries;
use ibis_core::source::load_library_snapshot;
use ibis_core::zotero::{self, libraries};
use ibis_zotero_local::LocalZotero;

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
/// holds the versions already loaded.
pub(super) fn load_zotero(
    chosen: ZoteroLibraries,
    previous: Option<&libraries::Snapshots>,
) -> Result<Loaded, LoadError> {
    let mut zotero = LocalZotero::default();
    let wanted: Vec<zotero::Library> = chosen.libraries().iter().copied().map(zotero_library).collect();
    match libraries::refresh(&mut zotero, &wanted, previous)? {
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
) -> Result<Loaded, LoadError> {
    match source {
        Source::File(path) => load_library(path, reload).map(|library| Loaded::New(Box::new(library))),
        Source::Zotero(chosen) => load_zotero(chosen, previous.as_ref().filter(|_| reload)),
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
}
