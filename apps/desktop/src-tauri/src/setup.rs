//! First-run setup: what Ibis can offer to read, found without reading it.
//!
//! Zotero is asked whether it is open through its local API root, which
//! reads no library. `.bib` files are found by name in the usual document
//! folders. The interface sees each file's name and folder name only, and
//! opens one by its list number and place in that list, so it never sends a
//! path to open and a choice from an older list is refused.

use crate::library::{Source, start_open};
use ibis_engine::{Cancellation, discovery, health::InspectionPolicy};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};

#[derive(Default)]
pub(crate) struct Offered(discovery::Discovery);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ZoteroPresence {
    Available,
    AccessOff,
    /// Not running, but its library is on this computer and can be read.
    Closed,
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

impl From<discovery::ZoteroPresence> for ZoteroPresence {
    fn from(found: discovery::ZoteroPresence) -> Self {
        match found {
            discovery::ZoteroPresence::Available => Self::Available,
            discovery::ZoteroPresence::AccessOff => Self::AccessOff,
            discovery::ZoteroPresence::Closed => Self::Closed,
            discovery::ZoteroPresence::Absent => Self::Absent,
        }
    }
}

/// What the welcome screen can say about Zotero before anything is chosen, so
/// it can offer Zotero first. Reads Zotero's profile configuration and no library.
#[tauri::command]
#[specta::specta]
pub(crate) async fn find_zotero() -> Result<ZoteroPresence, String> {
    tauri::async_runtime::spawn_blocking(|| {
        discovery::find_zotero(InspectionPolicy::Setup, true, &Cancellation::default())
    })
    .await
    .map_err(|_| "looking for Zotero stopped unexpectedly".to_string())?
    .map(ZoteroPresence::from)
    .map_err(|_| "looking for Zotero stopped unexpectedly".to_string())
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
    let request = app
        .state::<Offered>()
        .0
        .reserve()
        .map_err(|_| "looking for libraries stopped unexpectedly".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let (zotero, paths, cut_short) = request.find(InspectionPolicy::Setup, roots)?;
        let (list, files) = request.publish(paths, |path| FoundFile {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            folder: path
                .parent()
                .and_then(Path::file_name)
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        })?;
        Ok::<_, ibis_core::contract::EngineError>(Sources {
            zotero: zotero.into(),
            list,
            files,
            cut_short,
        })
    })
    .await
    .map_err(|_| "looking for libraries stopped unexpectedly".to_string())?
    .map_err(|_| "looking for libraries stopped unexpectedly".to_string())
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
        .resolve(list, index)
        .ok_or("That file is no longer offered; look for libraries again")?;
    start_open(&app, Source::File(path), true);
    Ok(())
}
