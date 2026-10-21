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

/// Validate and prepare from one locked snapshot; clipboard I/O happens afterward.
pub(crate) fn prepare_snapshot<T>(
    library: &Mutex<Library>,
    expected: &str,
    prepare: impl FnOnce(&Library) -> Result<T, String>,
) -> Result<T, String> {
    let locked = library.lock().map_err(|_| "library state is unavailable".to_string())?;
    locked.require_revision(expected)?;
    prepare(&locked)
}

/// Copy the selected records as a citation to the clipboard. Format choices
/// are saved separately when selected, even if no citation is copied.
#[tauri::command]
#[specta::specta]
pub(crate) fn copy_citation(
    ids: Vec<RecordId>,
    format: KeyFormat,
    expected_revision: String,
    app: AppHandle,
    library: State<'_, Mutex<Library>>,
) -> Result<Copied, String> {
    let rendered = prepare_snapshot(&library, &expected_revision, |locked| {
        render_citation(locked.index.references(), &ids, format)
    })?;
    app.clipboard()
        .write_text(rendered.text.clone())
        .map_err(|e| format!("the clipboard could not be written ({e})"))?;
    // Choices are saved when selected. A copy that was already in flight must
    // not replace a newer format choice when the clipboard finishes writing.
    Ok(Copied {
        text: rendered.text,
        warnings: rendered.warnings,
    })
}

/// The selected records as CSL-JSON, in the order given, for the citation
/// engine. Sent as JSON text so the fields stay exactly as the core writes them.
#[tauri::command]
#[specta::specta]
pub(crate) fn csl_items(
    ids: Vec<RecordId>,
    expected_revision: String,
    library: State<'_, Mutex<Library>>,
) -> Result<String, String> {
    prepare_snapshot(&library, &expected_revision, |locked| {
        csl_json(locked.index.references(), &ids)
    })
}

pub(crate) fn csl_json(references: &[Reference], ids: &[RecordId]) -> Result<String, String> {
    let items = ids
        .iter()
        .map(|id| {
            references
                .iter()
                .find(|r| &r.id == id)
                .map(Reference::to_csl_json)
                .ok_or_else(|| "a selected reference is no longer in the library".to_string())
        })
        .collect::<Result<Vec<_>, String>>()?;
    serde_json::to_string(&items).map_err(|_| "the references could not be prepared for formatting".to_string())
}

/// Largest formatted text accepted for the clipboard, far above any real
/// bibliography, so a runaway request cannot exhaust memory.
const MAX_FORMATTED: usize = 16 * 1024 * 1024;

/// Copy a formatted citation or bibliography as HTML with a plain-text
/// alternative, so word processors keep italics and plain editors get text.
#[tauri::command]
#[specta::specta]
pub(crate) fn copy_formatted(
    text: String,
    html: String,
    expected_revision: String,
    app: AppHandle,
    library: State<'_, Mutex<Library>>,
) -> Result<(), String> {
    write_formatted(&library, &expected_revision, text, html, |text, html| {
        app.clipboard()
            .write_html(html, Some(text))
            .map_err(|e| format!("the clipboard could not be written ({e})"))
    })
}

pub(crate) fn write_formatted(
    library: &Mutex<Library>,
    expected: &str,
    text: String,
    html: String,
    write: impl FnOnce(String, String) -> Result<(), String>,
) -> Result<(), String> {
    if text.len() + html.len() > MAX_FORMATTED {
        return Err("the formatted text is too large to copy".into());
    }
    prepare_snapshot(library, expected, |_| Ok(()))?;
    write(text, html)
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

    #[test]
    fn csl_items_follow_the_chosen_records_in_order() {
        let loaded = bibtex::load_str("a.bib", "@article{same, title={First}}\n@book{other, title={Other}}\n");
        let ids: Vec<RecordId> = loaded.references.iter().rev().map(|r| r.id.clone()).collect();
        let json = csl_json(&loaded.references, &ids).unwrap_or_else(|e| panic!("{e}"));
        let items: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap_or_else(|e| panic!("{e}"));
        let titles: Vec<_> = items.iter().map(|item| item["title"].as_str()).collect();
        assert_eq!(titles, [Some("Other"), Some("First")]);
        let missing = RecordId {
            source: "b.bib".into(),
            native_key: "same".into(),
        };
        assert!(csl_json(&loaded.references, &[missing]).is_err());
    }
}
