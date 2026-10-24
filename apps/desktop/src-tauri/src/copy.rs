//! Copying the selected references as a citation.

use std::sync::Mutex;

use ibis_core::cite::{self, CiteItem, KeyFormat};
use ibis_core::model::{RecordId, Reference};
use ibis_core::settings;
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::library::{CommandError, Library};
use crate::preferences::SettingsFile;

#[derive(Serialize, specta::Type)]
pub(crate) struct Copied {
    text: String,
    /// What the chosen syntax could not express, and other problems the user should see.
    warnings: Vec<String>,
}

impl From<cite::Rendered> for Copied {
    fn from(rendered: cite::Rendered) -> Self {
        Self {
            text: rendered.text,
            warnings: rendered.warnings,
        }
    }
}

/// Request-local display pieces; copied output keeps its existing payload.
#[derive(Serialize, specta::Type)]
pub(crate) struct CitationPreview {
    text: String,
    warnings: Vec<String>,
    segments: Vec<PreviewSegment>,
}

#[derive(Serialize, specta::Type)]
struct PreviewSegment {
    item: Option<usize>,
    text: String,
}

/// Render the selected records as one citation in `format`.
pub(crate) fn render_citation(
    references: &[Reference],
    ids: &[RecordId],
    format: KeyFormat,
) -> Result<cite::Rendered, String> {
    let items = citation_items(references, ids)?;
    cite::render(format, &items).map_err(|e| e.to_string())
}

fn citation_items(references: &[Reference], ids: &[RecordId]) -> Result<Vec<CiteItem>, String> {
    ids.iter()
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
        .collect()
}

/// Validate and prepare from one locked snapshot; clipboard I/O happens afterward.
pub(crate) fn prepare_snapshot<T>(
    library: &Mutex<Library>,
    expected: &str,
    prepare: impl FnOnce(&Library) -> Result<T, String>,
) -> Result<T, CommandError> {
    let locked = library
        .lock()
        .map_err(|_| CommandError::from("library state is unavailable"))?;
    locked.require_revision(expected)?;
    Ok(prepare(&locked)?)
}

/// Prepare preview and copy through the same checked snapshot and formatter.
fn prepare_citation(
    library: &Mutex<Library>,
    expected: &str,
    ids: &[RecordId],
    format: KeyFormat,
) -> Result<CitationPreview, CommandError> {
    prepare_snapshot(library, expected, |locked| {
        let items = citation_items(locked.index.references(), ids)?;
        let (rendered, segments) = cite::render_with_segments(format, &items).map_err(|e| e.to_string())?;
        Ok(CitationPreview {
            text: rendered.text,
            warnings: rendered.warnings,
            segments: segments
                .into_iter()
                .map(|part| PreviewSegment {
                    item: part.item,
                    text: part.text,
                })
                .collect(),
        })
    })
}

/// Preview a key citation without writing the clipboard or saving a choice.
#[tauri::command]
#[specta::specta]
pub(crate) fn citation_text(
    ids: Vec<RecordId>,
    format: KeyFormat,
    expected_revision: String,
    library: State<'_, Mutex<Library>>,
) -> Result<CitationPreview, CommandError> {
    prepare_citation(&library, &expected_revision, &ids, format)
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
) -> Result<Copied, CommandError> {
    let rendered = prepare_snapshot(&library, &expected_revision, |locked| {
        render_citation(locked.index.references(), &ids, format)
    })?;
    let rendered = Copied::from(rendered);
    app.clipboard()
        .write_text(rendered.text.clone())
        .map_err(|e| format!("the clipboard could not be written ({e})"))?;
    // Choices are saved when selected. A copy that was already in flight must
    // not replace a newer format choice when the clipboard finishes writing.
    Ok(rendered)
}

/// The selected records as CSL-JSON, in the order given, for the citation
/// engine. Sent as JSON text so the fields stay exactly as the core writes them.
#[tauri::command]
#[specta::specta]
pub(crate) fn csl_items(
    ids: Vec<RecordId>,
    expected_revision: String,
    library: State<'_, Mutex<Library>>,
) -> Result<String, CommandError> {
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
) -> Result<(), CommandError> {
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
) -> Result<(), CommandError> {
    if text.len() + html.len() > MAX_FORMATTED {
        return Err("the formatted text is too large to copy".into());
    }
    prepare_snapshot(library, expected, |_| Ok(()))?;
    Ok(write(text, html)?)
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
    use ibis_core::search::OwnedIndex;
    use std::sync::Arc;

    const FORMATS: [(KeyFormat, &str); 7] = [
        (KeyFormat::Pandoc, "[@other; @same]"),
        (KeyFormat::Autocite, "\\autocites{other}{same}"),
        (KeyFormat::Parencite, "\\parencites{other}{same}"),
        (KeyFormat::Textcite, "\\textcites{other}{same}"),
        (KeyFormat::Cite, "\\cite{other,same}"),
        (KeyFormat::Typst, "@other @same"),
        (KeyFormat::Keys, "other, same"),
    ];

    fn preview_library() -> (Mutex<Library>, Vec<RecordId>) {
        let mut loaded = bibtex::load_str(
            "synthetic",
            "@article{same, title={First}}\n@book{other, title={Second}}",
        );
        // Equal native keys across different sources must still resolve by full identity.
        for (index, reference) in loaded.references.iter_mut().enumerate() {
            reference.id = RecordId {
                source: format!("synthetic-{index}"),
                native_key: "shared".into(),
            };
        }
        let ids = loaded
            .references
            .iter()
            .rev()
            .map(|reference| reference.id.clone())
            .collect();
        let mut library = Library::default();
        library.source = Some(crate::library::Source::File("synthetic.bib".into()));
        library.index = Arc::new(OwnedIndex::new(loaded.references));
        (Mutex::new(library), ids)
    }

    #[test]
    fn key_preview_matches_copy_syntax_warnings_order_and_identity_for_all_formats() {
        let (library, ids) = preview_library();
        let before = library.lock().unwrap_or_else(|_| panic!("synthetic lock"));
        let index = Arc::clone(&before.index);
        let source = before.source.clone();
        drop(before);
        for (format, expected) in FORMATS {
            let preview = prepare_citation(&library, "0", &ids, format).unwrap_or_else(|error| panic!("{error}"));
            let copied = render_citation(index.references(), &ids, format).unwrap_or_else(|error| panic!("{error}"));
            assert_eq!(preview.text, expected);
            assert_eq!(preview.text, copied.text);
            assert_eq!(preview.warnings, copied.warnings);
            assert!(preview.warnings.is_empty());
            assert_eq!(
                preview
                    .segments
                    .iter()
                    .map(|part| part.text.as_str())
                    .collect::<String>(),
                expected
            );
            assert_eq!(
                preview.segments.iter().filter_map(|part| part.item).collect::<Vec<_>>(),
                [0, 1]
            );
        }
        let many_ids = ids.iter().cloned().cycle().take(50).collect::<Vec<_>>();
        let many =
            prepare_citation(&library, "0", &many_ids, KeyFormat::Keys).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(many.text, ["other", "same"].repeat(25).join(", "));
        assert!(library.try_lock().is_ok(), "preparation must release the snapshot lock");
        let after = library.lock().unwrap_or_else(|_| panic!("synthetic lock"));
        assert!(Arc::ptr_eq(&index, &after.index));
        assert_eq!(after.source, source);
        assert!(after.require_revision("0").is_ok());
    }

    #[test]
    fn key_preview_rejects_stale_or_unavailable_snapshot_before_record_lookup() {
        let (library, mut ids) = preview_library();
        ids[0].source = "missing".into();
        for (format, _) in FORMATS {
            assert_eq!(
                prepare_citation(&library, "retired", &ids, format).err(),
                Some(CommandError::SnapshotChanged)
            );
            assert_eq!(
                prepare_citation(&Mutex::new(Library::default()), "0", &ids, format).err(),
                Some(CommandError::SnapshotChanged)
            );
        }
    }

    #[test]
    fn key_preview_preserves_empty_missing_and_invalid_selection_errors() {
        let (library, _) = preview_library();
        let missing = RecordId {
            source: "missing".into(),
            native_key: "shared".into(),
        };
        for (format, _) in FORMATS {
            assert_eq!(
                prepare_citation(&library, "0", &[], format).err(),
                Some(CommandError::from("no references selected"))
            );
            assert_eq!(
                prepare_citation(&library, "0", std::slice::from_ref(&missing), format).err(),
                Some(CommandError::from("a selected reference is no longer in the library"))
            );
        }
        for key in [None, Some("invalid key"), Some("bad{key}")] {
            let (library, ids) = preview_library();
            let mut locked = library.lock().unwrap_or_else(|_| panic!("synthetic lock"));
            let mut references = locked.index.references().to_vec();
            references[0].citekey = key.map(str::to_string);
            locked.index = Arc::new(OwnedIndex::new(references));
            drop(locked);
            for (format, _) in FORMATS {
                assert!(prepare_citation(&library, "0", &ids, format).is_err());
            }
        }
    }

    #[test]
    fn key_preview_payload_keeps_nonempty_formatter_warnings() {
        let payload = Copied::from(cite::Rendered {
            text: "synthetic citation".into(),
            warnings: vec!["synthetic warning".into()],
        });
        let json = serde_json::to_value(payload).unwrap_or_else(|_| panic!("serializable citation"));
        assert_eq!(
            json,
            serde_json::json!({"text": "synthetic citation", "warnings": ["synthetic warning"]})
        );
    }

    #[test]
    fn key_preview_keeps_unicode_and_format_specific_validation() {
        for key in ["ключ", "a#key"] {
            let (library, ids) = preview_library();
            let index = {
                let mut locked = library.lock().unwrap_or_else(|_| panic!("synthetic lock"));
                let mut references = locked.index.references().to_vec();
                references[0].citekey = Some(key.into());
                locked.index = Arc::new(OwnedIndex::new(references));
                Arc::clone(&locked.index)
            };
            for (format, _) in FORMATS {
                let preview = prepare_citation(&library, "0", &ids, format);
                let copied = render_citation(index.references(), &ids, format);
                let latex = matches!(
                    format,
                    KeyFormat::Autocite | KeyFormat::Parencite | KeyFormat::Textcite | KeyFormat::Cite
                );
                assert_eq!(preview.is_ok(), key == "ключ" || !latex);
                match (preview, copied) {
                    (Ok(preview), Ok(copied)) => {
                        assert_eq!(preview.text, copied.text);
                        assert_eq!(preview.warnings, copied.warnings);
                    }
                    (Err(preview), Err(copied)) => assert_eq!(preview, CommandError::from(copied)),
                    _ => panic!("preview and copy validation diverged"),
                }
            }
        }
    }

    #[test]
    fn key_preview_reports_unavailable_lock_without_panicking() {
        let (library, ids) = preview_library();
        let poisoned = std::panic::catch_unwind(|| {
            let _guard = library.lock().unwrap_or_else(|_| panic!("synthetic lock"));
            panic!("synthetic poison");
        });
        assert!(poisoned.is_err());
        assert_eq!(
            prepare_citation(&library, "0", &ids, KeyFormat::Pandoc).err(),
            Some(CommandError::from("library state is unavailable"))
        );
    }

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
