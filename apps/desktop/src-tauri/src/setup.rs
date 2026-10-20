//! First-run setup: what Ibis can offer to read, found without reading it.
//!
//! Zotero is asked whether it is open through its local API root, which
//! reads no library. `.bib` files are found by name in the usual document
//! folders. The interface sees each file's name and folder name only, and
//! opens one by its list number and place in that list, so it never sends a
//! path to open and a choice from an older list is refused.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ibis_core::find;
use ibis_core::zotero::api::{self, Presence};
use ibis_zotero_local::LocalZotero;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::library::{Source, start_open};

/// The files last offered and that list's number, so a choice names one by
/// its place in the list it was shown from.
#[derive(Default)]
pub(crate) struct Offered(pub(crate) Mutex<OfferedList>);

#[derive(Default)]
pub(crate) struct OfferedList {
    list: u32,
    paths: Vec<PathBuf>,
}

impl OfferedList {
    /// Replace the list, returning its new number.
    fn offer(&mut self, paths: Vec<PathBuf>) -> u32 {
        self.list = self.list.wrapping_add(1);
        self.paths = paths;
        self.list
    }

    /// The file at `index` in list `list`, if that is still the list offered.
    fn chosen(&self, list: u32, index: u32) -> Option<PathBuf> {
        (list == self.list)
            .then(|| self.paths.get(index as usize).cloned())
            .flatten()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ZoteroPresence {
    Available,
    AccessOff,
    Absent,
}

/// A `.bib` file found on this computer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
pub(crate) struct FoundFile {
    /// The file name, such as `refs.bib`.
    pub(crate) name: String,
    /// The name of the folder holding it, without the rest of its path.
    pub(crate) folder: String,
}

/// What first-run setup can offer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
pub(crate) struct Sources {
    pub(crate) zotero: ZoteroPresence,
    /// The number to send back with a choice from `files`.
    pub(crate) list: u32,
    pub(crate) files: Vec<FoundFile>,
    /// Some folders were too large to search fully, or more files were
    /// found than are listed.
    pub(crate) cut_short: bool,
}

fn presence(found: Presence) -> ZoteroPresence {
    match found {
        Presence::Available => ZoteroPresence::Available,
        Presence::AccessOff => ZoteroPresence::AccessOff,
        Presence::Absent => ZoteroPresence::Absent,
    }
}

fn shown(path: &Path) -> FoundFile {
    let name = |p: Option<&Path>| {
        p.and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    FoundFile {
        name: name(Some(path)),
        folder: name(path.parent()),
    }
}

/// Find what setup can offer: whether Zotero is open, and `.bib` files in
/// the documents, desktop and downloads folders.
#[tauri::command]
#[specta::specta]
pub(crate) async fn find_sources(app: AppHandle) -> Result<Sources, String> {
    let paths = app.path();
    let roots: Vec<PathBuf> = [paths.document_dir(), paths.desktop_dir(), paths.download_dir()]
        .into_iter()
        .flatten()
        .collect();
    let (zotero, found) = tauri::async_runtime::spawn_blocking(move || {
        (api::presence(&mut LocalZotero::default()), find::bib_files(&roots))
    })
    .await
    .map_err(|_| "looking for libraries stopped unexpectedly".to_string())?;
    let paths: Vec<PathBuf> = found.files.into_iter().map(|f| f.path).collect();
    let files = paths.iter().map(|p| shown(p)).collect();
    let list = app
        .state::<Offered>()
        .0
        .lock()
        .map_err(|_| "looking for libraries stopped unexpectedly".to_string())?
        .offer(paths);
    Ok(Sources {
        zotero: presence(zotero),
        list,
        files,
        cut_short: found.cut_short,
    })
}

/// Open the file at `index` in list `list` from [`find_sources`] and
/// remember it; the result arrives as `library:changed`. A choice from a
/// list that has since been replaced is refused.
#[tauri::command]
#[specta::specta]
pub(crate) fn open_found_library(
    app: AppHandle,
    offered: State<'_, Offered>,
    list: u32,
    index: u32,
) -> Result<(), String> {
    let path = offered
        .0
        .lock()
        .ok()
        .and_then(|offered| offered.chosen(list, index))
        .ok_or("That file is no longer offered; look for libraries again")?;
    start_open(&app, Source::File(path), true);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_found_file_is_shown_by_name_and_folder_name_only() {
        let path = PathBuf::from("/synthetic/home/Documents/thesis/refs.bib");
        assert_eq!(
            shown(&path),
            FoundFile {
                name: "refs.bib".into(),
                folder: "thesis".into()
            }
        );
        assert_eq!(presence(Presence::AccessOff), ZoteroPresence::AccessOff);
    }

    #[test]
    fn a_choice_from_an_older_list_is_refused() {
        let mut offered = OfferedList::default();
        let first = offered.offer(vec![PathBuf::from("/synthetic/a.bib")]);
        let second = offered.offer(vec![PathBuf::from("/synthetic/b.bib")]);
        assert_eq!(offered.chosen(first, 0), None);
        assert_eq!(offered.chosen(second, 0), Some(PathBuf::from("/synthetic/b.bib")));
        assert_eq!(offered.chosen(second, 1), None);
    }
}
