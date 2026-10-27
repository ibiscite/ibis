//! Copying the selected references as a citation.

use std::sync::Mutex;

use ibis_core::cite::KeyFormat;
use ibis_core::settings;
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::library::{CommandError, Library};
use crate::preferences::SettingsFile;

#[derive(Serialize, specta::Type)]
pub(crate) struct Copied {
    pub(crate) text: String,
    /// What the chosen syntax could not express, and other problems the user should see.
    pub(crate) warnings: Vec<String>,
}

/// The format the picker starts with: the last one chosen, or Pandoc.
#[tauri::command]
#[specta::specta]
pub(crate) fn copy_format(settings_file: State<'_, SettingsFile>) -> KeyFormat {
    settings::load(&settings_file.path)
        .settings
        .copy_format
        .unwrap_or(KeyFormat::Pandoc)
}

#[cfg(test)]
mod engine_file_acceptance;

#[tauri::command]
#[specta::specta]
pub(crate) async fn preview_file_citation(
    items: Vec<crate::library::engine_file::FileSelection>,
    format: KeyFormat,
    style: Option<settings::CitationStyle>,
    expected_revision: String,
    library: State<'_, Mutex<Library>>,
) -> Result<crate::library::engine_file::FilePreview, CommandError> {
    let file = {
        let library = library
            .lock()
            .map_err(|_| CommandError::from("The library is unavailable."))?;
        let _ = expected_revision;
        if library.opening {
            return Err("The library is loading. Try again after it opens.".into());
        }
        library
            .file
            .clone()
            .ok_or_else(|| CommandError::from("The selected file is no longer open."))?
    };
    let reservation = file.reserve_preview();
    tauri::async_runtime::spawn_blocking(move || file.preview(reservation, items, format, style))
        .await
        .map_err(|_| CommandError::from("The preview is unavailable. Try again."))?
}
#[tauri::command]
#[specta::specta]
pub(crate) fn copy_file_citation(
    ticket: String,
    content_hash: String,
    app: AppHandle,
    library: State<'_, Mutex<Library>>,
) -> Result<Copied, CommandError> {
    let library = library.lock().map_err(|_| CommandError::SnapshotChanged)?;
    if library.opening {
        return Err("The library is loading. Try again after it opens.".into());
    }
    library
        .file
        .as_ref()
        .ok_or(CommandError::SnapshotChanged)?
        .copy(&ticket, &content_hash, |content| {
            if let Some(html) = content.html {
                app.clipboard().write_html(html, Some(content.text))
            } else {
                app.clipboard().write_text(content.text)
            }
            .map_err(|_| "The clipboard could not be written.".to_string())
        })
}
