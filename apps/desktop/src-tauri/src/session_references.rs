//! "Cited since Ibis opened": what Ibis copied since it started or since
//! a reset. Ibis cannot see a document, so this is never called its bibliography.
//!
//! The list holds record identities and nothing else, in memory. It is written
//! nowhere unless the person exports the bibliography, and it never reaches a
//! library. Text comes from the open library each time it is shown.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::library::commands::RecentIdentity;
use crate::library::engine_file::{FileLibrary, SessionBibliography};
use crate::library::{CommandError, Library, SearchResult};
use crate::picker::ChooserOpen;

const FAILED: &str = "The references could not be read. Try again.";
pub(crate) use ibis_engine::recents::SessionReferences;
use ibis_engine::recents::{List, Undo};

/// The edit Undo would reverse, for its label. Only the removed row is named.
#[derive(Serialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(crate) enum UndoView {
    Removed { reference: Option<Box<SearchResult>> },
    Reset,
}

#[derive(Serialize, schemars::JsonSchema)]
pub(crate) struct SessionReferencesView {
    pub(crate) revision: u32,
    /// The listed references that are in the open library.
    pub(crate) rows: Vec<SearchResult>,
    /// Listed references from a library that is not open, left out of the
    /// bibliography until it is.
    pub(crate) absent: u32,
    /// The list reached its limit, so a later citation was left off.
    pub(crate) overflowed: bool,
    pub(crate) undo: Option<UndoView>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(crate) enum SessionEdit {
    Remove { id: RecentIdentity },
    Reset,
    Undo,
}

fn open_file(library: &Mutex<Library>) -> Option<std::sync::Arc<FileLibrary>> {
    library.lock().ok()?.file.clone()
}

fn view(list: &List, file: Option<&FileLibrary>) -> Result<SessionReferencesView, String> {
    let rows = file
        .map(|file| file.session_rows(&list.items))
        .transpose()?
        .unwrap_or_default();
    let undo = list.undo.last().map(|undo| match undo {
        Undo::Removed { item, .. } => UndoView::Removed {
            reference: file
                .and_then(|file| file.session_rows(std::slice::from_ref(item)).ok())
                .and_then(|mut rows| rows.pop())
                .map(Box::new),
        },
        Undo::Reset { .. } => UndoView::Reset,
    });
    Ok(SessionReferencesView {
        revision: list.revision,
        absent: (list.items.len() - rows.len()) as u32,
        rows,
        overflowed: list.overflowed,
        undo,
    })
}

#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) fn session_references(
    session: State<'_, SessionReferences>,
    library: State<'_, Mutex<Library>>,
) -> Result<SessionReferencesView, String> {
    let file = open_file(&library);
    view(&session.lock(), file.as_deref())
}

#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) fn edit_session_references(
    edit: SessionEdit,
    session: State<'_, SessionReferences>,
    library: State<'_, Mutex<Library>>,
) -> Result<SessionReferencesView, String> {
    let file = open_file(&library);
    let mut list = session.lock();
    match edit {
        // An identity from before the library changed names nothing listed.
        SessionEdit::Remove {
            id: RecentIdentity(Some(id)),
        } => list.remove(&id),
        SessionEdit::Remove { .. } => {}
        SessionEdit::Reset => list.reset(),
        SessionEdit::Undo => list.undo(),
    }
    view(&list, file.as_deref())
}

/// The style an output id names: only a listed style writes a bibliography.
fn style_of(id: &str) -> Result<String, CommandError> {
    match crate::output::listed(id).map(crate::output::OutputRef::spec) {
        Some(ibis_core::contract::OutputSpec::Csl { style }) => Ok(style.clone()),
        _ => Err(CommandError::from("Choose a citation style to write a bibliography.")),
    }
}

/// The list as a formatted bibliography, for the person to look at before
/// copying it. `None` when nothing listed is in the open library.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn session_bibliography(
    style: String,
    session: State<'_, SessionReferences>,
    library: State<'_, Mutex<Library>>,
) -> Result<Option<SessionBibliography>, CommandError> {
    // With no library open nothing listed can be written, which is no failure.
    let Some(file) = open_file(&library) else {
        return Ok(None);
    };
    let items = session.items();
    let style = style_of(&style)?;
    tauri::async_runtime::spawn_blocking(move || file.session_bibliography(&items, &style))
        .await
        .map_err(|_| CommandError::from(FAILED))?
}

/// Copy what was shown. Prepared again from the list as it is now, and refused
/// unless that is exactly what the person saw.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn copy_session_bibliography(
    style: String,
    content_hash: String,
    app: AppHandle,
    session: State<'_, SessionReferences>,
    library: State<'_, Mutex<Library>>,
) -> Result<(), CommandError> {
    // Formatting can take a while: anything copied meanwhile is newer and stays.
    let began = crate::copy::clipboard_mark();
    let style = style_of(&style)?;
    let file = open_file(&library).ok_or(CommandError::SnapshotChanged)?;
    let items = session.items();
    let content = tauri::async_runtime::spawn_blocking(move || {
        file.commit_session_bibliography(&items, &style, &content_hash, |content| {
            Ok(crate::copy::OwnedContent::of(content))
        })
    })
    .await
    .map_err(|_| CommandError::from(FAILED))??;
    let (send, written) = std::sync::mpsc::sync_channel(1);
    let main = app.clone();
    app.run_on_main_thread(move || {
        let _ = send.send(crate::copy::write_copy_only_unless_written(&main, &content, began));
    })
    .map_err(|_| CommandError::from(FAILED))?;
    tauri::async_runtime::spawn_blocking(move || written.recv())
        .await
        .map_err(|_| CommandError::from(FAILED))?
        .map_err(|_| CommandError::from(FAILED))?
}

/// The kinds of file a bibliography can be saved as, by its extension.
fn export_text(extension: &str, content: &ibis_core::contract::ClipboardContent<'_>) -> Option<String> {
    match extension.to_ascii_lowercase().as_str() {
        "rtf" => content.rtf.map(str::to_owned),
        "html" | "htm" => content.html.map(|html| {
            format!(
                "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>References cited since Ibis opened</title></head><body>{html}</body></html>\n"
            )
        }),
        "txt" => Some(format!("{}\n", content.text.trim_end())),
        _ => None,
    }
}

/// Save what was shown, as rich text, a web page or plain text. This is the one
/// way the list leaves memory, and only because the person asked. `None` when
/// the dialog was cancelled; otherwise the file's name.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn export_session_bibliography(
    style: String,
    content_hash: String,
    window: WebviewWindow,
    session: State<'_, SessionReferences>,
    library: State<'_, Mutex<Library>>,
) -> Result<Option<String>, CommandError> {
    const NOT_SAVED: &str = "The bibliography could not be saved. Try again.";
    let style = style_of(&style)?;
    let file = open_file(&library).ok_or(CommandError::SnapshotChanged)?;
    let items = session.items();
    let app = window.app_handle().clone();
    // The dialog belongs to the picker, which hides when it loses focus unless
    // a chooser holds it open; hiding would take away the dialog's parent.
    let owner = app
        .state::<ChooserOpen>()
        .begin()
        .ok_or_else(|| CommandError::from(NOT_SAVED))?;
    #[cfg(target_os = "macos")]
    crate::panel::activate_for_dialog(&app);
    let generation = app.state::<crate::picker::PickerGeneration>().current();
    let dialog = app
        .dialog()
        .file()
        .set_parent(&window)
        .set_title("Save References Cited Since Ibis Opened")
        .set_file_name("Cited since Ibis opened.rtf")
        .add_filter("Rich Text", &["rtf"])
        .add_filter("Web page", &["html"])
        .add_filter("Plain text", &["txt"]);
    let chosen = tauri::async_runtime::spawn_blocking(move || owner.during(|| dialog.blocking_save_file())).await;
    let handle = app.clone();
    crate::diagnostics::report(
        app.run_on_main_thread(move || {
            if let Some(window) = handle.get_webview_window("main") {
                crate::picker::reconcile_suppressed_blur(&window, generation);
            }
        }),
        crate::diagnostics::NativeIssue::Window,
    );
    let Some(chosen) = chosen.map_err(|_| CommandError::from(NOT_SAVED))? else {
        return Ok(None);
    };
    let path = chosen.into_path().map_err(|_| CommandError::from(NOT_SAVED))?;
    // The dialog asked about the name it returned, not one Ibis completes.
    let (path, replace) = if path.extension().is_some() {
        (path, true)
    } else {
        (path.with_extension("rtf"), false)
    };
    let shown = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    tauri::async_runtime::spawn_blocking(move || {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default();
        let text = file.commit_session_bibliography(&items, &style, &content_hash, |content| {
            export_text(extension, &content).ok_or_else(|| "Save it as .rtf, .html or .txt.".to_owned())
        })?;
        ibis_core::settings::backup::save(&path, &text, replace).map_err(|error| {
            CommandError::from(if error.kind() == std::io::ErrorKind::AlreadyExists {
                "A file with that name is already there. Choose another name."
            } else {
                NOT_SAVED
            })
        })
    })
    .await
    .map_err(|_| CommandError::from(NOT_SAVED))??;
    Ok(Some(shown))
}

/// Apply every saved privacy change, including delayed off notifications.
pub(crate) fn manage<R: tauri::Runtime>(app: &AppHandle<R>, settings_path: &std::path::Path, enabled: bool) {
    use tauri::Listener;
    *app.state::<SessionReferences>().lock() = List::open(settings_path.with_file_name("recents.json"), enabled);
    if !enabled {
        let _ = app.state::<SessionReferences>().flush();
    }
    if let Some(window) = app.get_webview_window("main") {
        let app = app.clone();
        let settings_path = settings_path.to_owned();
        window.listen("preferences:changed", move |event| {
            if let Some(on) = serde_json::from_str::<serde_json::Value>(event.payload())
                .ok()
                .and_then(|value| value.get("recent_citations").and_then(serde_json::Value::as_bool))
            {
                let history = app.state::<SessionReferences>();
                let loaded = ibis_core::settings::load(&settings_path);
                let saved = loaded.problem.is_none() && loaded.settings.recent_citations.unwrap_or(true);
                if !on && saved {
                    let _ = history.forget();
                }
                let _ = history.set_enabled(saved);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::contract::ClipboardContent;

    #[test]
    fn an_export_gives_each_kind_of_file_or_refuses_one_it_cannot_write() {
        let content = ClipboardContent {
            text: "Taylor, A. (2026). Synthetic signals.\n",
            html: Some("<p>Taylor</p>"),
            rtf: Some("{\\rtf1 Taylor}"),
        };
        assert_eq!(export_text("RTF", &content).as_deref(), Some("{\\rtf1 Taylor}"));
        assert_eq!(
            export_text("txt", &content).as_deref(),
            Some("Taylor, A. (2026). Synthetic signals.\n")
        );
        let page = export_text("html", &content).unwrap_or_default();
        assert!(page.contains("<p>Taylor</p>") && page.contains("charset=\"utf-8\""));
        assert!(
            page.contains("References cited since Ibis opened"),
            "never titled as a document's bibliography"
        );
        assert_eq!(export_text("pdf", &content), None);
        let plain = ClipboardContent {
            html: None,
            rtf: None,
            ..content
        };
        assert_eq!(export_text("rtf", &plain), None);
    }
}

#[cfg(test)]
mod bibliography_tests {
    use crate::library::CommandError;
    use crate::library::engine_file::FileLibrary;
    use ibis_core::contract::ItemRef;
    use ibis_engine::Cancellation;

    const LIBRARY: &str = "@book{taylor2026,title={Synthetic signals},author={Taylor, Avery},year={2026},publisher={Imaginary Press}}\n\
                           @book{morgan2025,title={Synthetic noise},author={Morgan, Ann},year={2025},publisher={Invented Letters}}\n";

    fn opened(text: &str) -> (tempfile::TempDir, FileLibrary, Vec<ItemRef>) {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = dir.path().join("refs.bib");
        std::fs::write(&path, text).unwrap_or_else(|error| panic!("{error}"));
        let file = FileLibrary::open(&path).unwrap_or_else(|_| panic!("a library"));
        let query = serde_json::from_value(serde_json::json!({"text":"synthetic","interpret_locators":false}))
            .unwrap_or_else(|error| panic!("{error}"));
        let reply = file
            .search(&query, 10, None, &Cancellation::default())
            .unwrap_or_else(|error| panic!("{error}"));
        let rows = serde_json::to_value(&reply).unwrap_or_else(|error| panic!("{error}"));
        let ids = rows["results"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| serde_json::from_value(row["id"].clone()).ok())
                    .collect()
            })
            .unwrap_or_default();
        (dir, file, ids)
    }

    fn shown(file: &FileLibrary, ids: &[ItemRef]) -> crate::library::engine_file::SessionBibliography {
        match file.session_bibliography(ids, "apa") {
            Ok(Some(bibliography)) => bibliography,
            _ => panic!("a bibliography"),
        }
    }

    #[test]
    fn the_bibliography_is_formatted_as_rich_and_plain_text_together() {
        let (_dir, file, ids) = opened(LIBRARY);
        let bibliography = shown(&file, &ids);
        assert!(bibliography.text.contains("Taylor, A. (2026)"), "{}", bibliography.text);
        assert!(bibliography.text.contains("Morgan, A. (2025)"), "{}", bibliography.text);
        let written = file
            .commit_session_bibliography(&ids, "apa", &bibliography.content_hash, |content| {
                Ok((
                    content.text.to_owned(),
                    content.html.map(str::to_owned),
                    content.rtf.map(str::to_owned),
                ))
            })
            .unwrap_or_else(|_| panic!("a commit"));
        assert_eq!(written.0, bibliography.text, "what was shown is what is copied");
        assert!(written.1.is_some_and(|html| html.contains("<i>Synthetic signals</i>")));
        assert!(written.2.is_some());
    }

    #[test]
    fn a_list_edited_since_it_was_shown_is_refused_not_copied() {
        let (_dir, file, ids) = opened(LIBRARY);
        let bibliography = shown(&file, &ids);
        let mut wrote = false;
        let refused = file.commit_session_bibliography(&ids[..1], "apa", &bibliography.content_hash, |_| {
            wrote = true;
            Ok(())
        });
        assert!(matches!(refused, Err(CommandError::SnapshotChanged)));
        assert!(!wrote);
    }

    #[test]
    fn references_from_a_library_that_is_not_open_are_left_out_never_guessed() {
        let (_dir, file, ids) = opened(LIBRARY);
        let (_other_dir, other, _) = opened("@book{other2026,title={Synthetic other}}\n");
        assert_eq!(other.session_rows(&ids).map(|rows| rows.len()), Ok(0));
        assert!(matches!(other.session_bibliography(&ids, "apa"), Ok(None)));
        assert_eq!(file.session_rows(&ids).map(|rows| rows.len()), Ok(2));
    }

    #[test]
    fn a_reference_the_style_cannot_write_is_named_and_the_rest_still_come() {
        let (_dir, file, ids) = opened(
            "@book{others2026,title={Synthetic signals},author={Taylor, Avery and others},year={2026}}\n\
             @book{morgan2025,title={Synthetic noise},author={Morgan, Ann},year={2025}}\n",
        );
        let bibliography = shown(&file, &ids);
        assert_eq!(bibliography.left_out, ["others2026"], "{}", bibliography.text);
        assert!(bibliography.text.contains("Morgan, A. (2025)"), "{}", bibliography.text);
        assert!(!bibliography.text.contains("Taylor"), "{}", bibliography.text);
        let written = file.commit_session_bibliography(&ids, "apa", &bibliography.content_hash, |content| {
            Ok(content.text.to_owned())
        });
        assert_eq!(
            written.ok().as_deref(),
            Some(bibliography.text.as_str()),
            "what was shown is what is copied"
        );
    }

    #[test]
    fn a_list_the_style_cannot_write_at_all_says_which_references() {
        let (_dir, file, ids) =
            opened("@book{others2026,title={Synthetic signals},author={Taylor, Avery and others},year={2026}}\n");
        match file.session_bibliography(&ids, "apa") {
            Err(CommandError::Failed { message }) => assert!(message.contains("others2026"), "{message}"),
            _ => panic!("a refusal that names the reference"),
        }
    }

    #[test]
    fn a_warning_names_its_reference_in_the_sheet() {
        let (_dir, file, ids) =
            opened("@book{taylor2026,title={Synthetic Signals In Title Case},author={Taylor, Avery},year={2026}}\n");
        let bibliography = shown(&file, &ids);
        assert!(
            bibliography
                .warnings
                .iter()
                .any(|warning| warning.contains("the title of taylor2026")),
            "{:?}",
            bibliography.warnings
        );
    }

    #[test]
    fn a_numeric_style_gives_its_full_entries() {
        let (_dir, file, ids) = opened(LIBRARY);
        let bibliography = match file.session_bibliography(&ids, "ieee") {
            Ok(Some(bibliography)) => bibliography,
            _ => panic!("a bibliography"),
        };
        assert!(bibliography.text.contains("Synthetic signals"), "{}", bibliography.text);
    }
}

#[cfg(test)]
mod privacy_tests;
