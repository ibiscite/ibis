//! "Cited since opening": what Ibis copied since it started or since
//! a reset. Ibis cannot see a document, so this is never called its bibliography.
//!
//! The list holds record identities and nothing else, in memory. It is written
//! nowhere unless the person exports the bibliography, and it never reaches a
//! library. Text comes from the open library each time it is shown.

use ibis_engine::citations::{Operation, SessionId};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::library::commands::RecentIdentity;
use crate::library::engine_library::FileLibrary;
use crate::library::{CommandError, Library, SearchResult};
use crate::picker::ChooserOpen;
use ibis_core::contract::SessionBibliography;

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

fn view(list: &List, file: Option<&FileLibrary>) -> Result<SessionReferencesView, CommandError> {
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
) -> Result<SessionReferencesView, CommandError> {
    let file = open_file(&library);
    view(&session.lock(), file.as_deref())
}

#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) fn edit_session_references(
    edit: SessionEdit,
    session: State<'_, SessionReferences>,
    library: State<'_, Mutex<Library>>,
) -> Result<SessionReferencesView, CommandError> {
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
        _ => Err(CommandError::Presented(
            ibis_core::contract::CitationError::UnsupportedOutput
                .problem()
                .recover_with(ibis_core::contract::Recovery::Close)
                .on(ibis_core::contract::CitationSurface::Bibliography),
        )),
    }
}

/// The list as a formatted bibliography, for the person to look at before
/// copying it. `None` when nothing listed is in the open library.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn session_bibliography(
    style: String,
    intent: crate::library::PreviewIntent,
    session: State<'_, SessionReferences>,
    library: State<'_, Mutex<Library>>,
) -> Result<Option<SessionBibliography>, CommandError> {
    // With no library open nothing listed can be written, which is no failure.
    let Some(file) = open_file(&library) else {
        return Ok(None);
    };
    let cancel = file
        .engine
        .citations()
        .start(SessionId::PICKER, Operation::Bibliography, intent)?;
    let items = session.items();
    let style = style_of(&style)?;
    let task = file
        .engine
        .citations()
        .bibliography(SessionId::PICKER, items, style, cancel, file.sample)?;
    tauri::async_runtime::spawn_blocking(move || task.run().map_err(CommandError::from))
        .await
        .map_err(|_| CommandError::BibliographyUnavailable)?
}

/// Copy what was shown. Prepared again from the list as it is now, and refused
/// unless that is exactly what the person saw.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn copy_session_bibliography(
    style: String,
    content_hash: String,
    intent: crate::library::PreviewIntent,
    app: AppHandle,
    session: State<'_, SessionReferences>,
    library: State<'_, Mutex<Library>>,
) -> Result<(), CommandError> {
    // Formatting can take a while: anything copied meanwhile is newer and stays.
    let began = crate::copy::clipboard_mark();
    let style = style_of(&style)?;
    let file = open_file(&library).ok_or(CommandError::SnapshotChanged)?;
    let cancel = file
        .engine
        .citations()
        .start(SessionId::PICKER, Operation::Bibliography, intent)?;
    let formatting = cancel.clone();
    let (revision, items) = {
        let list = session.lock();
        (list.revision, list.items.clone())
    };
    let checked = tauri::async_runtime::spawn_blocking(move || {
        file.engine
            .citations()
            .commit_bibliography(SessionId::PICKER, &items, &style, &content_hash, &formatting)
            .map_err(CommandError::from)
    })
    .await
    .map_err(|_| CommandError::BibliographyUnavailable)??;
    let (send, written) = std::sync::mpsc::sync_channel(1);
    let main = app.clone();
    app.run_on_main_thread(move || {
        let result = cancel.check().map_err(CommandError::from).and_then(|()| {
            commit_list_copy(&main.state(), revision, || {
                let file = open_file(&main.state()).ok_or(CommandError::SnapshotChanged)?;
                let guard = checked.check(&file.engine)?;
                crate::copy::write_copy_only_unless_written(
                    &main,
                    &crate::copy::OwnedContent::of(guard.content()),
                    began,
                )
            })
        });
        let _ = send.send(result);
    })
    .map_err(|_| CommandError::BibliographyUnavailable)?;
    tauri::async_runtime::spawn_blocking(move || written.recv())
        .await
        .map_err(|_| CommandError::BibliographyUnavailable)?
        .map_err(|_| CommandError::BibliographyUnavailable)?
}

/// Keep edits out until the write finishes; a superseded list never reaches
/// the clipboard, even if its formatter has already finished.
fn commit_list_copy<T>(
    session: &SessionReferences,
    revision: u32,
    write: impl FnOnce() -> Result<T, CommandError>,
) -> Result<T, CommandError> {
    let list = session.lock();
    if list.revision != revision {
        return Err(CommandError::SnapshotChanged);
    }
    write()
}

/// One of the app's own lines from the catalogue.
fn words(id: &str) -> String {
    ibis_engine::catalogue::text(id, None, &[]).unwrap_or_default()
}

/// The kinds of file a bibliography can be saved as, by its extension.
fn export_text(extension: &str, content: &ibis_core::contract::ClipboardContent<'_>) -> Result<String, CommandError> {
    match extension.to_ascii_lowercase().as_str() {
        "rtf" => content.rtf.map(str::to_owned),
        "html" | "htm" => content.html.map(|html| {
            let title = words("app-saved-page");
            format!(
                "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>{title}</title></head><body>{html}</body></html>\n"
            )
        }),
        "txt" => Some(format!("{}\n", content.text.trim_end())),
        _ => None,
    }.ok_or(CommandError::BibliographyFormat)
}

/// Save what was shown, as rich text, a web page or plain text. This is the one
/// way the list leaves memory, and only because the person asked. `None` when
/// the dialog was cancelled; otherwise the file's name.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn export_session_bibliography(
    style: String,
    content_hash: String,
    intent: crate::library::PreviewIntent,
    window: WebviewWindow,
    session: State<'_, SessionReferences>,
    library: State<'_, Mutex<Library>>,
) -> Result<Option<String>, CommandError> {
    let style = style_of(&style)?;
    let file = open_file(&library).ok_or(CommandError::SnapshotChanged)?;
    let cancel = file
        .engine
        .citations()
        .start(SessionId::PICKER, Operation::Bibliography, intent)?;
    let items = session.items();
    let app = window.app_handle().clone();
    // The dialog belongs to the picker, which hides when it loses focus unless
    // a chooser holds it open; hiding would take away the dialog's parent.
    let owner = app
        .state::<ChooserOpen>()
        .begin()
        .ok_or(CommandError::BibliographyNotSaved)?;
    #[cfg(target_os = "macos")]
    crate::panel::activate_for_dialog(&app);
    let generation = app.state::<crate::picker::PickerGeneration>().current();
    let dialog = app
        .dialog()
        .file()
        .set_parent(&window)
        .set_title(words("app-saved-title"))
        .set_file_name(format!("{}.rtf", words("app-saved-file")))
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
    let Some(chosen) = chosen.map_err(|_| CommandError::BibliographyNotSaved)? else {
        return Ok(None);
    };
    cancel.check().map_err(CommandError::from)?;
    let path = chosen.into_path().map_err(|_| CommandError::BibliographyNotSaved)?;
    // The dialog asked about the name it returned, not one Ibis completes.
    let (path, replace) = export_path(path);
    let shown = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    tauri::async_runtime::spawn_blocking(move || {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default();
        let result =
            file.engine
                .citations()
                .commit_bibliography(SessionId::PICKER, &items, &style, &content_hash, &cancel)?;
        let current = open_file(&app.state()).ok_or(CommandError::SnapshotChanged)?;
        let guard = result.check(&current.engine)?;
        let content = guard.content();
        let text = export_text(extension, &content)?;
        cancel.check().map_err(CommandError::from)?;
        save_export(&path, &text, replace)
    })
    .await
    .map_err(|_| CommandError::BibliographyNotSaved)??;
    Ok(Some(shown))
}

fn export_path(path: std::path::PathBuf) -> (std::path::PathBuf, bool) {
    if path.extension().is_some() {
        (path, true)
    } else {
        (path.with_extension("rtf"), false)
    }
}

/// Save only the name the dialog confirmed; a completed extension cannot overwrite.
fn save_export(path: &std::path::Path, text: &str, replace: bool) -> Result<(), CommandError> {
    ibis_engine::settings::backup::save(path, text, replace).map_err(|error| match error.kind() {
        std::io::ErrorKind::AlreadyExists => CommandError::BibliographyNameTaken,
        _ => CommandError::BibliographyNotSaved,
    })
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
            let change = serde_json::from_str::<serde_json::Value>(event.payload()).unwrap_or_default();
            // Only a save that set the switch says anything about it; the rest
            // of a snapshot may be older than the switch.
            let chose = change
                .get("fields")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|fields| fields.iter().any(|field| field == "recent_citations"));
            if let Some(on) = change
                .get("recent_citations")
                .and_then(serde_json::Value::as_bool)
                .filter(|_| chose)
            {
                let history = app.state::<SessionReferences>();
                let loaded = ibis_engine::settings::load(&settings_path);
                let saved = loaded
                    .cites()
                    .is_ok_and(|settings| settings.portable.recent_citations.unwrap_or(true));
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
    fn an_extensionless_bibliography_collision_offers_another_name() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let chosen = dir.path().join("invented-bibliography");
        std::fs::write(chosen.with_extension("rtf"), "existing")?;
        let (path, replace) = export_path(chosen);
        let error = match save_export(&path, "new bibliography", replace) {
            Err(error) => error,
            Ok(()) => panic!("an appended extension overwrote an unconfirmed file"),
        };
        assert_eq!(
            error.to_string(),
            "A file with that name is already there. Choose another name."
        );
        let problem = serde_json::to_value(error)?;
        assert_eq!(problem["recovery"]["action"], "retry");
        assert_eq!(problem["message"]["id"], "app-bibliography-name-taken");
        assert_eq!(std::fs::read_to_string(path)?, "existing");
        Ok(())
    }

    #[test]
    fn resetting_a_list_during_formatting_preserves_the_empty_list_and_clipboard()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("invented.bib");
        std::fs::write(
            &path,
            "@book{taylor,title={Synthetic signals},author={Taylor, Alex},year={2026}}",
        )?;
        let file =
            FileLibrary::open(&path, &ibis_engine::Cancellation::default()).map_err(|_| "open invented library")?;
        let query = serde_json::from_value(serde_json::json!({"text":"signals","interpret_locators":false}))?;
        let rows = serde_json::to_value(file.search(&query, 10, None, &ibis_engine::Cancellation::default())?)?;
        let id = serde_json::from_value(rows["results"][0]["id"].clone())?;
        let session = SessionReferences::default();
        session.lock().add([id]);
        let (revision, items) = {
            let list = session.lock();
            (list.revision, list.items.clone())
        };
        let shown = file
            .engine
            .citations()
            .bibliography(
                ibis_engine::citations::SessionId::PICKER,
                items.clone(),
                "apa".into(),
                ibis_engine::Cancellation::default(),
                file.sample,
            )?
            .run()?
            .ok_or("bibliography")?;
        session.lock().reset();
        let content = file
            .engine
            .citations()
            .commit_bibliography(
                ibis_engine::citations::SessionId::PICKER,
                &items,
                "apa",
                &shown.content_hash,
                &ibis_engine::Cancellation::default(),
            )?
            .result()
            .text
            .clone();
        let mut clipboard = "Invented original clipboard".to_owned();
        let copied = commit_list_copy(&session, revision, || {
            clipboard = content;
            Ok(())
        });
        assert_eq!(copied, Err(CommandError::SnapshotChanged));
        assert!(session.items().is_empty());
        assert_eq!(clipboard, "Invented original clipboard");

        // Undo restores identical content, but does not revive the old copy.
        session.lock().undo();
        assert_eq!(
            commit_list_copy(&session, revision, || Ok(())),
            Err(CommandError::SnapshotChanged)
        );
        // A fresh request can write; failed clipboard I/O remains a failure.
        let revision = session.lock().revision;
        commit_list_copy(&session, revision, || {
            clipboard = shown.text.clone();
            Ok(())
        })
        .map_err(|error| error.to_string())?;
        assert_eq!(clipboard, shown.text);
        let failed = CommandError::ClipboardUnavailable;
        assert_eq!(
            commit_list_copy(&session, revision, || Err::<(), _>(failed.clone())),
            Err(failed)
        );
        assert_eq!(clipboard, shown.text);
        Ok(())
    }

    #[test]
    fn an_export_gives_each_kind_of_file_or_refuses_one_it_cannot_write() {
        let content = ClipboardContent {
            text: "Taylor, A. (2026). Synthetic signals.\n",
            html: Some("<p>Taylor</p>"),
            rtf: Some("{\\rtf1 Taylor}"),
        };
        assert_eq!(export_text("RTF", &content).as_deref(), Ok("{\\rtf1 Taylor}"));
        assert_eq!(
            export_text("txt", &content).as_deref(),
            Ok("Taylor, A. (2026). Synthetic signals.\n")
        );
        let page = export_text("html", &content).unwrap_or_default();
        assert!(page.contains("<p>Taylor</p>") && page.contains("charset=\"utf-8\""));
        assert!(
            page.contains("References cited since opening"),
            "never titled as a document's bibliography"
        );
        let error = export_text("pdf", &content)
            .err()
            .unwrap_or_else(|| panic!("unsupported format"));
        let problem = serde_json::to_value(error).unwrap_or_default();
        assert_eq!(problem["code"], "bibliography-format");
        assert_eq!(problem["recovery"]["action"], "retry");
        assert_eq!(problem["focus"], "recovery-action");
        let plain = ClipboardContent {
            html: None,
            rtf: None,
            ..content
        };
        assert_eq!(export_text("rtf", &plain), Err(CommandError::BibliographyFormat));
    }
}

#[cfg(test)]
mod privacy_tests;
