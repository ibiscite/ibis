//! One observation per presentation, retained until the webview is listening.

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use super::PickerGeneration;
use crate::diagnostics::{NativeIssue, report};
use crate::library::{CommandError, LibraryStatus};

#[derive(Serialize, schemars::JsonSchema)]
pub(crate) struct PickerSnapshot {
    pub(crate) presentation: u64,
    pub(crate) selection: Option<String>,
    pub(crate) library: LibraryStatus,
    pub(crate) recents: Result<(crate::library::search::SearchReply, bool, bool), CommandError>,
    pub(crate) destination: Result<crate::destination::PickerDestination, CommandError>,
    pub(crate) preferences: Result<crate::preferences::PickerPreferences, CommandError>,
}

#[derive(Default)]
pub(crate) struct Presentation(Mutex<Pending>);

#[derive(Default)]
struct Pending {
    listening: bool,
    snapshot: Option<Arc<PickerSnapshot>>,
    captured: Option<String>,
    reveal: Option<(u64, bool)>,
}

fn build<R: tauri::Runtime>(
    app: &AppHandle<R>,
    presentation: u64,
    selection: Option<String>,
    captured: Option<&str>,
    destination: Result<crate::destination::PickerDestination, CommandError>,
) -> PickerSnapshot {
    let library = app.state::<ibis_engine::library::Owner>();
    let (mut status, file) = library.status_with_current();
    status.settings_notice = app.state::<crate::preferences::SettingsFile>().notice();
    let project = app.state::<crate::project::ProjectBibliography>().current();
    PickerSnapshot {
        presentation,
        selection,
        library: status,
        recents: crate::library::commands::recent_rows(
            &app.state::<crate::session_references::SessionReferences>(),
            captured,
            file.as_deref(),
            project.as_ref(),
        ),
        destination,
        preferences: app.state::<crate::preferences::SettingsFile>().preferences(),
    }
}

pub(super) fn prepare(app: &AppHandle, presentation: u64, selection: Option<String>, focus: bool) {
    let editor = app.state::<crate::editor_endpoint::Pending>();
    let captured = editor.app(crate::paste::last_app(app).map(|(id, _, _)| id.normalized()));
    let destination = crate::app_context::refresh(app, crate::paste::last_app(app).as_ref().map(|(id, _, _)| id))
        .map(|context| crate::destination::present(app, &context));
    let snapshot = Arc::new(build(app, presentation, selection, captured.as_deref(), destination));
    if !app.state::<PickerGeneration>().owns(presentation) {
        return;
    }
    let state = app.state::<Presentation>();
    let mut pending = state.0.lock().unwrap_or_else(|e| e.into_inner());
    pending.reveal = Some((presentation, focus));
    pending.snapshot = Some(Arc::clone(&snapshot));
    pending.captured = captured;
    if pending.listening {
        report(
            app.emit_to("main", "picker:preparing", &snapshot),
            NativeIssue::PickerEvent,
        );
    }
}

impl Presentation {
    fn subscribe<R: tauri::Runtime>(&self, app: &AppHandle<R>, owner: u64) -> (Option<Arc<PickerSnapshot>>, bool) {
        let mut pending = self.0.lock().unwrap_or_else(|e| e.into_inner());
        pending.listening = true;
        let snapshot = pending
            .snapshot
            .as_ref()
            .filter(|snapshot| snapshot.presentation == owner)
            .cloned();
        let preparing = pending.reveal.map(|(presentation, _)| presentation) == Some(owner);
        let captured = pending.captured.clone();
        drop(pending);
        // The library can finish opening before the webview can hear its event.
        // Subscribe first, then refresh its status and rows together while keeping
        // the selection and destination captured before the picker took focus.
        let snapshot = snapshot.filter(|_| preparing).map(|snapshot| {
            Arc::new(build(
                app,
                owner,
                snapshot.selection.clone(),
                captured.as_deref(),
                snapshot.destination.clone(),
            ))
        });
        (snapshot, preparing)
    }
}

/// Subscribe before reading: an opening before the webview mounted is retained here.
#[tauri::command]
#[ibis_desktop_macros::command(main)]
pub(crate) fn picker_snapshot(app: AppHandle, state: State<'_, Presentation>) -> Option<Arc<PickerSnapshot>> {
    let owner = app.state::<PickerGeneration>().current();
    let (snapshot, preparing) = state.subscribe(&app, owner);
    if preparing {
        return snapshot;
    }
    // A reloaded visible webview gets a fresh observation of the same target.
    // A hidden launch waits for an actual summon.
    if !app
        .get_webview_window("main")
        .is_some_and(|window| window.is_visible().unwrap_or(false))
    {
        return None;
    }
    let editor = app.state::<crate::editor_endpoint::Pending>();
    let captured = editor.app(crate::paste::last_app(&app).map(|(id, _, _)| id.normalized()));
    let destination = crate::app_context::refresh(&app, crate::paste::last_app(&app).as_ref().map(|(id, _, _)| id))
        .map(|context| crate::destination::present(&app, &context));
    let snapshot = Arc::new(build(&app, owner, None, captured.as_deref(), destination));
    app.state::<PickerGeneration>().owns(owner).then_some(snapshot)
}

/// The complete snapshot has reached the DOM; only its presentation may reveal.
#[tauri::command]
#[ibis_desktop_macros::command(main)]
pub(crate) fn picker_rendered(app: AppHandle, presentation: u64) {
    let handle = app.clone();
    report(
        app.run_on_main_thread(move || {
            let state = handle.state::<Presentation>();
            let focus = {
                let mut pending = state.0.lock().unwrap_or_else(|e| e.into_inner());
                if pending.reveal.map(|(owner, _)| owner) != Some(presentation) {
                    return;
                }
                pending.reveal.take().map(|(_, focus)| focus)
            };
            if let Some(focus) = focus.filter(|_| handle.state::<PickerGeneration>().owns(presentation)) {
                super::reveal_snapshot(&handle, presentation, focus);
            }
        }),
        NativeIssue::Show,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::engine_library::FileLibrary;
    use crate::preferences::SettingsFile;
    use crate::session_references::SessionReferences;
    use ibis_engine::settings::Density;

    #[test]
    fn picker_snapshot_refreshes_a_library_loaded_before_subscription_and_keeps_its_destination()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let settings = SettingsFile::new(directory.path().join("settings.json"));
        settings.update(|saved| {
            saved.portable.output = Some("apa".into());
            saved.portable.density = Some(Density::Compact);
            saved.portable.larger_text = Some(true);
        })?;
        let path = directory.path().join("invented.bib");
        std::fs::write(
            &path,
            "@book{one,title={Invented first}}\n@book{two,title={Invented second}}",
        )?;
        let (finish, loading) = std::sync::mpsc::channel();
        let loading = Mutex::new(loading);
        let library = ibis_engine::library::Owner::new(
            Arc::new(settings.clone()),
            move || {
                loading
                    .lock()
                    .unwrap()
                    .recv_timeout(std::time::Duration::from_secs(10))
                    .unwrap();
                Ok(ibis_engine::Engine::new()?.without_file_watches())
            },
            |_| {},
        );
        library.submit(ibis_engine::library::Intent::Open {
            source: path.into(),
            save: ibis_engine::library::Save::Never,
            profile: None,
        });
        let session = SessionReferences::default();
        session.set_enabled(true)?;
        let app = tauri::test::mock_builder()
            .manage(settings)
            .manage(library)
            .manage(session)
            .manage(crate::project::ProjectBibliography::default())
            .manage(Presentation::default())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))?;
        let destination = crate::destination::PickerDestination {
            app: Some("Invented Writer".into()),
            app_id: Some("invented.writer".into()),
            app_identity: None,
            format: ibis_engine::format::choose_with(
                &ibis_engine::settings::Settings::default(),
                None,
                Some(&ibis_core::contract::OutputSpec::Key {
                    syntax: ibis_core::contract::KeySyntax::Pandoc,
                    command: None,
                }),
            ),
            usual_format: "apa".into(),
            find_open: false,
            unreadable: false,
            can_paste: true,
            paste: None,
            paste_question: false,
            can_choose_paste: false,
            writing: None,
            context_version: None,
            writing_context: None,
            offers_projects: false,
        };
        // The first observation can precede the load and the webview's event
        // listeners. Its destination and selection still belong to this summon.
        let opening = Arc::new(build(
            app.handle(),
            7,
            Some("Invented selection".into()),
            Some("invented.writer"),
            Ok(destination.clone()),
        ));
        assert_eq!(opening.library.state, crate::library::LibraryState::Loading);
        assert!(!opening.library.has_snapshot);
        {
            let state = app.state::<Presentation>();
            let mut pending = state.0.lock().unwrap_or_else(|e| e.into_inner());
            pending.snapshot = Some(opening);
            pending.captured = Some("invented.writer".into());
            pending.reveal = Some((7, true));
        }
        let library = app.state::<ibis_engine::library::Owner>();
        finish.send(())?;
        assert!(library.wait_idle(std::time::Duration::from_secs(10)));
        let file = library.current().ok_or("library")?;
        let rows = file.search(
            &serde_json::from_value(serde_json::json!({"text":"Invented", "interpret_locators":false}))?,
            20,
            None,
            &ibis_engine::Cancellation::default(),
        )?;
        let rows = serde_json::to_value(rows)?;
        let id = serde_json::from_value::<ibis_core::contract::ItemRef>(rows["results"][1]["id"].clone())?;
        let session = app.state::<SessionReferences>();
        let epoch = session.lock().epoch();
        session.record(&epoch, Some("invented.writer"), std::slice::from_ref(&id));
        // Reloading the webview before reveal must refresh the library each time.
        for _ in 0..2 {
            let (observed, preparing) = app.state::<Presentation>().subscribe(app.handle(), 7);
            assert!(preparing);
            let snapshot = observed.ok_or("opening snapshot")?;
            assert_eq!(snapshot.library.state, crate::library::LibraryState::Loaded);
            assert_eq!(snapshot.presentation, 7);
            assert_eq!(snapshot.selection.as_deref(), Some("Invented selection"));
            assert_eq!(snapshot.library.references, 2);
            let (recents, history, failed) = snapshot.recents.as_ref().map_err(Clone::clone)?;
            let recents = serde_json::to_value(recents)?;
            assert_eq!(recents["revision"], snapshot.library.revision);
            assert_eq!(recents["results"][0]["id"], serde_json::to_value(&id)?);
            assert!(*history && !failed);
            assert_eq!(snapshot.destination.as_ref().map_err(Clone::clone)?, &destination);
            let preferences = snapshot.preferences.as_ref().map_err(Clone::clone)?;
            assert_eq!(preferences.output.id(), "apa");
            assert_eq!(preferences.density, Density::Compact);
            assert!(preferences.larger_text);
        }
        let (observed, preparing) = app.state::<Presentation>().subscribe(app.handle(), 8);
        assert!(observed.is_none() && !preparing);
        Ok(())
    }
}
