//! Copying the selected references as a citation.

use std::sync::Mutex;

use ibis_core::cite::{self, CiteItem, KeyFormat};
use ibis_core::model::{RecordId, Reference};
use ibis_core::settings;
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::library::Library;
use crate::preferences::SettingsFile;

#[derive(Serialize, specta::Type)]
pub(crate) struct Copied {
    text: String,
    /// What the chosen syntax could not express, and other problems the user should see.
    warnings: Vec<String>,
}

/// Render the selected records as one citation in `format`.
pub(crate) fn render_citation(
    references: &[Reference],
    ids: &[RecordId],
    format: KeyFormat,
) -> Result<cite::Rendered, String> {
    let items = ids
        .iter()
        .map(|id| {
            let reference = references
                .iter()
                .find(|r| &r.id == id)
                .ok_or_else(|| "a selected reference is no longer in the library".to_string())?;
            let key = reference.citekey.clone().ok_or_else(|| {
                format!(
                    "\"{}\" has no citation key, so it cannot be cited in this format",
                    reference.title_plain().unwrap_or_else(|| id.native_key.clone())
                )
            })?;
            Ok(CiteItem {
                key,
                ..CiteItem::default()
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    cite::render(format, &items).map_err(|e| e.to_string())
}

/// Copy the selected records as a citation to the clipboard. The format is
/// remembered for next time.
#[tauri::command]
#[specta::specta]
pub(crate) fn copy_citation(
    ids: Vec<RecordId>,
    format: KeyFormat,
    app: AppHandle,
    library: State<'_, Mutex<Library>>,
    settings_file: State<'_, SettingsFile>,
) -> Result<Copied, String> {
    let rendered = {
        let library = library.lock().map_err(|_| "library state is unavailable".to_string())?;
        render_citation(library.index.references(), &ids, format)?
    };
    app.clipboard()
        .write_text(rendered.text.clone())
        .map_err(|e| format!("the clipboard could not be written ({e})"))?;
    let mut warnings = rendered.warnings;
    // Read and saved each time, so edits made to the file meanwhile are kept
    // and a failed save is reported on every copy.
    if let Err(e) = settings_file.update(|s| s.copy_format = Some(format)) {
        warnings.push(format!("the format choice could not be saved: {e}"));
    }
    Ok(Copied {
        text: rendered.text,
        warnings,
    })
}

/// The format the picker starts with: the last one used, or Pandoc.
#[tauri::command]
#[specta::specta]
pub(crate) fn copy_format(settings_file: State<'_, SettingsFile>) -> KeyFormat {
    settings::load(&settings_file.path)
        .settings
        .copy_format
        .unwrap_or(KeyFormat::Pandoc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::bibtex;

    #[test]
    fn citations_are_rendered_by_record_identity_not_by_key() {
        let loaded = bibtex::load_str(
            "a.bib",
            "@article{same, title={From A}}\n@article{other, title={Other}}\n",
        );
        let ids: Vec<RecordId> = loaded.references.iter().rev().map(|r| r.id.clone()).collect();
        let rendered = render_citation(&loaded.references, &ids, KeyFormat::Pandoc).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(rendered.text, "[@other; @same]");
        let missing = RecordId {
            source: "b.bib".into(),
            native_key: "same".into(),
        };
        assert!(render_citation(&loaded.references, &[missing], KeyFormat::Pandoc).is_err());
    }
}
