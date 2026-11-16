//! Native bibliography chooser and clipboard effects.
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use serde::Serialize;
use ibis_engine::{EntryRefusal, EntryResult};
use ibis_engine::library::Owner;
use crate::diagnostics::{NativeIssue, report};
use crate::library::commands::RecentIdentity;
use crate::library::engine_library::FileLibrary;
use crate::picker::ChooserOpen;
pub(crate) use ibis_engine::app_context::project::{Current, ProjectBibliography, preview_warnings, preview_evidence};

pub(crate) fn watch_openings(app: &AppHandle) {
    use tauri::Listener;
    let project = app.state::<ProjectBibliography>().inner().clone();
    app.listen("picker:shown", move |_| project.recheck());
}

/// Choose the file with the system dialog; the interface never handles a path.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) fn choose_project(app: AppHandle) {
    let Some(owner) = app.state::<ChooserOpen>().begin() else {
        return;
    };
    #[cfg(target_os = "macos")]
    crate::panel::activate_for_dialog(&app);
    let generation = app.state::<crate::picker::PickerGeneration>().current();
    let Ok(bibliography_owner) = crate::app_context::bibliography_owner(&app) else {
        let _ = app.emit("project:not-saved", ());
        return;
    };
    let mut dialog = app
        .dialog()
        .file()
        .set_title("Choose the writing's bibliography")
        .add_filter("BibTeX and BibLaTeX", &["bib", "bibtex", "biblatex"]);
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }
    let handle = app.clone();
    dialog.pick_file(move |picked| {
        owner.during(|| {
            if let Some(Ok(path)) = picked.map(|path| path.into_path())
                && crate::app_context::set_bibliography(&handle, &bibliography_owner, Some(path)).is_err()
            {
                report(handle.emit("project:not-saved", ()), NativeIssue::LibraryEvent);
            }
        });
        let owner = handle.clone();
        report(
            handle.run_on_main_thread(move || {
                if let Some(window) = owner.get_webview_window("main") {
                    crate::picker::reconcile_suppressed_blur(&window, generation);
                }
            }),
            NativeIssue::Window,
        );
    });
}

/// How an entry copy ended; a refusal says how to get the entry another way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum EntryCopy {
    Copied,
    NoOriginal,
    /// The file or the choice changed since this was offered.
    Changed,
    Repeated,
    Dependency,
}

impl From<EntryRefusal> for EntryCopy {
    fn from(refusal: EntryRefusal) -> Self {
        match refusal {
            EntryRefusal::NoOriginal => Self::NoOriginal,
            EntryRefusal::Changed => Self::Changed,
            EntryRefusal::Repeated => Self::Repeated,
            EntryRefusal::Dependency => Self::Dependency,
        }
    }
}

/// Copy a reference's original entry with its key, for adding to the project's
/// file. The text goes to the clipboard here and never through the web view.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn copy_project_entry(
    id: RecentIdentity,
    generation: u32,
    app: AppHandle,
    library: State<'_, Owner>,
) -> Result<EntryCopy, crate::library::CommandError> {
    let file = library.current().ok_or(crate::library::CommandError::SnapshotChanged)?;
    // An identity from before the library changed names nothing now.
    let Some(id) = id.0 else {
        return Ok(EntryCopy::Changed);
    };
    let result = tauri::async_runtime::spawn_blocking(move || file.original_entry(&id))
        .await
        .map_err(|_| crate::errors::NativeOperation::CopyProjectEntry.failure())??;
    let text = match result {
        EntryResult::Entry { text, .. } => text,
        EntryResult::Refused { reason } => return Ok(reason.into()),
    };
    // Checked last: the choice may have changed during that read.
    let captured = crate::paste::last_app(&app);
    crate::app_context::refresh(&app, captured.as_ref().map(|(id, _, _)| id))?;
    app.state::<ProjectBibliography>()
        .commit_generation(generation, || {
            crate::copy::write_plain(&app, &text).map_err(|_| crate::library::CommandError::ClipboardUnavailable)?;
            Ok(EntryCopy::Copied)
        })
        .unwrap_or(Ok(EntryCopy::Changed))
}

