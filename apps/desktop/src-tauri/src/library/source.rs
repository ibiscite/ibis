//! Where references come from, and how a source is named and saved.

use std::path::PathBuf;

use ibis_core::settings::{LibrarySetting, ZoteroLibraries, ZoteroLibrary};
use ibis_core::zotero;

/// Where references come from: a BibTeX or BibLaTeX file, or Zotero
/// libraries read through Zotero's local API while Zotero runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    File(PathBuf),
    Zotero(ZoteroLibraries),
}

impl From<PathBuf> for Source {
    fn from(path: PathBuf) -> Self {
        Source::File(path)
    }
}

impl From<LibrarySetting> for Source {
    fn from(setting: LibrarySetting) -> Self {
        match setting {
            LibrarySetting::Bibtex(path) => Source::File(path),
            LibrarySetting::Zotero(library) => Source::Zotero(library),
        }
    }
}

impl Source {
    pub(super) fn setting(&self) -> LibrarySetting {
        match self {
            Source::File(path) => LibrarySetting::Bibtex(path.clone()),
            Source::Zotero(libraries) => LibrarySetting::Zotero(libraries.clone()),
        }
    }

    /// The name shown to the user: a file name only, never a full path.
    pub(super) fn name(&self) -> Option<String> {
        match self {
            Source::File(path) => path.file_name().map(|n| n.to_string_lossy().into_owned()),
            Source::Zotero(libraries) => Some(match libraries.libraries() {
                [ZoteroLibrary::User] => "Zotero library".to_string(),
                [ZoteroLibrary::Group(id)] => format!("Zotero group {id}"),
                several => format!("{} Zotero libraries", several.len()),
            }),
        }
    }
}

pub(super) fn zotero_library(library: ZoteroLibrary) -> zotero::Library {
    match library {
        ZoteroLibrary::User => zotero::Library::User,
        ZoteroLibrary::Group(id) => zotero::Library::Group(id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use crate::library::state::{Library, LoadRequests, begin_open, candidate};
    use crate::library::status::status;

    #[test]
    fn several_zotero_libraries_are_named_listed_and_saved_in_order() {
        let chosen = ZoteroLibraries::new(vec![ZoteroLibrary::Group(7), ZoteroLibrary::User])
            .unwrap_or_else(|| panic!("valid choice"));
        let source = Source::Zotero(chosen.clone());
        assert_eq!(source.name().as_deref(), Some("2 Zotero libraries"));
        assert_eq!(source.setting(), LibrarySetting::Zotero(chosen.clone()));
        assert_eq!(Source::from(source.setting()), source);

        let library = Mutex::new(Library::default());
        let requests = LoadRequests::default();
        let (_, _, opening) =
            begin_open(&library, &requests, source).unwrap_or_else(|| panic!("the choice should start"));
        assert!(opening.zotero);
        assert_eq!(opening.zotero_libraries, [ZoteroLibrary::Group(7), ZoteroLibrary::User]);
        let file = candidate(PathBuf::from("refs.bib"), ibis_core::bibtex::load_str("synthetic", ""));
        assert!(status(&file).zotero_libraries.is_empty());
    }
}
