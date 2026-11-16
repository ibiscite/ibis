//! Desktop library intents, native effects and picker rows. The engine owns the library.

pub(crate) mod attachment_text;
pub(crate) mod commands;
pub(crate) mod engine_library;
mod error;
pub(crate) mod open_source;
#[cfg(test)]
mod samples;
mod search;
#[cfg(test)]
pub(crate) mod tests;
#[cfg(test)]
#[path = "../../../../../crates/ibis-engine/tests/support/zotero.rs"]
mod zotero_fixture;

use crate::picker::{ChooserOpen, reveal_picker};
use crate::preferences::SettingsFile;
pub(crate) use error::CommandError;
use ibis_engine::library::{Intent, Owner, Save};
pub(crate) use ibis_engine::library::{LibraryState, LibraryStatus, Source};
use ibis_engine::settings::{LibrarySetting, ZoteroLibraries, ZoteroLibrary};
pub(crate) use search::{PreviewIntent, ProjectMark, SearchRequests, SearchResult};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

pub(crate) fn refresh_settings_notice<R: tauri::Runtime>(app: &AppHandle<R>, settings_file: &SettingsFile) {
    if settings_file.refresh_notice()
        && let Some(owner) = app.try_state::<Owner>()
    {
        owner.republish();
    }
}

pub(crate) fn start_open(app: &AppHandle, source: Source, remember: bool) {
    app.state::<Owner>().submit(Intent::Open {
        source,
        save: if remember { Save::Always } else { Save::IfChanged },
        profile: None,
    });
}

pub(crate) fn apply_file_settings(app: &AppHandle, source: Option<LibrarySetting>, profile: Option<String>) {
    app.state::<Owner>().submit(match source {
        Some(source) => Intent::Open {
            source: source.into(),
            save: Save::Never,
            profile,
        },
        None => Intent::Close,
    });
}

pub(crate) fn use_zotero_profile(app: &AppHandle, name: String) {
    app.state::<Owner>().submit(Intent::Open {
        source: Source::Zotero(current_zotero_choice(app)),
        save: Save::Always,
        profile: Some(name),
    });
}

/// Whether a dropped file's name says it is a library.
pub(crate) fn accepts_library_file(path: &std::path::Path) -> bool {
    ibis_engine::names_a_library(path)
}

/// Show the system file dialog and open the chosen library. Cancelling
/// changes nothing.
pub(crate) fn choose_library_file(app: &AppHandle) {
    // The dialog belongs to the picker: show the picker (the request may come
    // from the tray), attach the dialog to it, and keep the picker open while
    // the dialog has the focus.
    let Some(owner) = app.state::<ChooserOpen>().begin() else {
        return;
    };
    reveal_picker(app);
    // The chooser belongs to this presentation; a newer one is never hidden.
    let generation = app.state::<crate::picker::PickerGeneration>().current();
    #[cfg(target_os = "macos")]
    crate::panel::activate_for_dialog(app);
    let mut dialog = app
        .dialog()
        .file()
        .set_title(ibis_engine::catalogue::text("app-file-dialog-title", None, &[]).unwrap_or_default())
        .add_filter(
            ibis_engine::catalogue::text("app-file-dialog-formats", None, &[]).unwrap_or_default(),
            &ibis_engine::library_file_extensions().collect::<Vec<_>>(),
        );
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }
    let handle = app.clone();
    dialog.pick_file(move |picked| {
        owner.during(|| match picked.map(|p| p.into_path()) {
            None => {}
            Some(Ok(path)) => start_open(&handle, Source::File(path), true),
            Some(Err(_)) => {
                handle.state::<Owner>().submit(Intent::RefuseNotLocal);
            }
        });
        // With the lease released, hide the picker if the person went elsewhere
        // while the chooser was open.
        let owner = handle.clone();
        crate::diagnostics::report(
            handle.run_on_main_thread(move || {
                if let Some(window) = owner.get_webview_window("main") {
                    crate::picker::reconcile_suppressed_blur(&window, generation);
                }
            }),
            crate::diagnostics::NativeIssue::Window,
        );
    });
}

fn current_zotero_choice(app: &AppHandle) -> ZoteroLibraries {
    app.state::<Owner>()
        .view()
        .zotero_choice()
        .unwrap_or_else(|| ZoteroLibrary::User.into())
}

pub(crate) fn use_zotero_library(app: &AppHandle) {
    start_open(app, Source::Zotero(current_zotero_choice(app)), true);
    reveal_picker(app);
}

pub(crate) fn refresh_zotero(app: &AppHandle) {
    app.state::<Owner>().submit(Intent::Check);
}

pub(crate) fn retry_zotero_on_focus(app: &AppHandle) {
    app.state::<Owner>().submit(Intent::RetryFailed);
}

pub(crate) fn current_status(app: &AppHandle) -> LibraryStatus {
    let mut status = app.state::<Owner>().status();
    status.settings_notice = app.state::<SettingsFile>().notice();
    status
}

pub(crate) fn reload_library_file(app: &AppHandle) {
    if !app.state::<Owner>().submit(Intent::Reload) {
        choose_library_file(app);
    }
}

/// Local profile discovery for the tray; call off the main thread.
pub(crate) fn zotero_detected() -> bool {
    engine_library::zotero_profiles().is_ok()
}
