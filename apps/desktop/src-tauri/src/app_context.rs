//! Capture native identity and attach delivery facts to the engine observation.
use crate::library::CommandError;
use ibis_core::destination::AppId;
use ibis_engine::app_context::{Capture, ContextRequest, ContextView, EditorContext, Owner};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

fn capture(app: &AppHandle, captured: Option<&AppId>) -> Capture {
    Capture {
        app: captured.cloned(),
        generation: app
            .try_state::<crate::picker::PickerGeneration>()
            .map_or(0, |visits| visits.current()),
        editor: app
            .try_state::<crate::editor_endpoint::Pending>()
            .and_then(|pending| pending.current())
            .map(|request| EditorContext {
                output: request.output,
                writing: request.writing,
            }),
    }
}
fn current(app: &AppHandle) -> Capture {
    capture(app, crate::paste::last_app(app).as_ref().map(|(id, _, _)| id))
}

pub(crate) fn manage<R: tauri::Runtime>(app: &AppHandle<R>, settings: &crate::preferences::SettingsFile) {
    let retire = app.clone();
    let publish = app.clone();
    let project = ibis_engine::app_context::project::ProjectBibliography::new(
        move || {
            let _ = crate::paste::change_destination(&retire, true, |_| Ok(()));
        },
        move |version| {
            let _ = publish.emit("project:changed", version);
        },
    );
    app.manage(project.clone());
    app.manage(Owner::new(settings.owner.clone(), project));
}

#[derive(Serialize, schemars::JsonSchema)]
pub(crate) struct AppContextView {
    destination: crate::destination::PickerDestination,
    writings: ibis_engine::app_context::WritingsView,
    project: Option<ibis_engine::app_context::project::ProjectStatus>,
}

/// One typed request for the captured app's engine context.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn app_context(
    app: AppHandle,
    owner: State<'_, Owner>,
    request: ContextRequest,
) -> Result<AppContextView, CommandError> {
    let captured = current(&app);
    let changed = matches!(
        request,
        ContextRequest::EditWriting { .. } | ContextRequest::StopProject {}
    );
    let view = owner.request(&captured, request, || current(&app) == captured);
    if changed {
        let _ = app.emit("writings:changed", ());
    }
    let view = view?;
    let destination = crate::destination::present(&app, &view);
    if current(&app) != captured {
        return Err(CommandError::SnapshotChanged);
    }
    Ok(AppContextView {
        destination,
        writings: view.writings,
        project: view.project,
    })
}

pub(crate) fn refresh(app: &AppHandle, captured: Option<&AppId>) -> Result<ContextView, CommandError> {
    let captured = capture(app, captured);
    app.state::<Owner>()
        .request(&captured, ContextRequest::Destination {}, || current(app) == captured)
        .map_err(Into::into)
}

pub(crate) fn bibliography_owner(app: &AppHandle) -> Result<ibis_engine::app_context::BibliographyOwner, CommandError> {
    let captured = current(app);
    app.state::<Owner>()
        .bibliography_owner(&captured, || current(app) == captured)
        .map_err(Into::into)
}
pub(crate) fn set_bibliography(
    app: &AppHandle,
    owner: &ibis_engine::app_context::BibliographyOwner,
    path: Option<std::path::PathBuf>,
) -> Result<(), CommandError> {
    let captured = current(app);
    let result = app
        .state::<Owner>()
        .set_bibliography(&captured, owner, path, || current(app) == captured)
        .map_err(Into::into);
    let _ = app.emit("writings:changed", ());
    result
}
