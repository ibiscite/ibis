//! Copying the selected references as a citation.

use std::sync::Mutex;

use ibis_core::cite::KeyFormat;
use ibis_core::settings;
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::library::{CommandError, Library, PreviewIntent, PreviewRequests};
use crate::preferences::SettingsFile;

#[derive(Serialize, specta::Type)]
pub(crate) struct Copied {
    pub(crate) text: String,
    /// What the chosen syntax could not express, and other problems the user should see.
    pub(crate) warnings: Vec<String>,
    /// Unique clipboard-write identity for receipts, even without a native paste proof.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub(crate) delivery: Option<u32>,
}

/// Allocate under the platform's write lock, before clipboard I/O. Exhaustion
/// refuses the write instead of wrapping or publishing an unidentified copy.
pub(crate) fn write_token() -> Result<u32, String> {
    static NEXT: Mutex<u32> = Mutex::new(0);
    let failed = || "The clipboard could not be written.".to_owned();
    let mut token = NEXT.lock().map_err(|_| failed())?;
    *token = token.checked_add(1).ok_or_else(failed)?;
    Ok(*token)
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

mod clipboard;
#[cfg(test)]
mod engine_file_acceptance;

#[tauri::command]
#[specta::specta]
pub(crate) async fn preview_file_citation(
    items: Vec<crate::library::engine_file::FileSelection>,
    format: KeyFormat,
    style: Option<settings::CitationStyle>,
    expected_revision: String,
    intent: PreviewIntent,
    library: State<'_, Mutex<Library>>,
    requests: State<'_, PreviewRequests>,
) -> Result<crate::library::engine_file::FilePreview, CommandError> {
    let cancel = requests
        .start(intent)
        .ok_or_else(|| CommandError::from("A newer citation preview replaced this request."))?;
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
    let reservation = file.reserve_preview_with(cancel);
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
    #[cfg(target_os = "windows")]
    let attempt = crate::paste::begin_copy(&app);
    // The library lock is held only to take the engine; the engine's own
    // commit barrier guards the write.
    let file = {
        let library = library.lock().map_err(|_| CommandError::SnapshotChanged)?;
        if library.opening {
            return Err("The library is loading. Try again after it opens.".into());
        }
        library.file.clone().ok_or(CommandError::SnapshotChanged)?
    };
    let mut delivery = None;
    let mut copied = file.copy(&ticket, &content_hash, |content| {
        #[cfg(target_os = "windows")]
        {
            let (token, sequence) = clipboard::write(&app, content)?;
            delivery = crate::paste::remember_copy(&app, attempt, sequence, token);
        }
        #[cfg(not(target_os = "windows"))]
        {
            delivery = clipboard::write(&app, content)?;
        }
        Ok(())
    })?;
    copied.delivery = delivery;
    Ok(copied)
}
