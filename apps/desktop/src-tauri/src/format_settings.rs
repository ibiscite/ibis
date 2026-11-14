//! Settings views contain names and format IDs, never bibliography paths.
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::{library::CommandError, preferences::SettingsFile};

#[derive(Serialize, schemars::JsonSchema)]
pub(crate) struct AppFormat {
    id: String,
    app: String,
    format: String,
}

#[derive(Serialize, schemars::JsonSchema)]
pub(crate) struct FormatSettingsView {
    apps: Vec<AppFormat>,
    writings: Vec<crate::writings::WritingView>,
    key_output_visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    unknown: Option<serde_json::Map<String, serde_json::Value>>,
}

#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) fn format_settings(app: AppHandle) -> Result<FormatSettingsView, CommandError> {
    let loaded = ibis_engine::settings::load(&app.state::<SettingsFile>().path);
    if let Some(problem) = loaded.problem {
        return Err(CommandError::from(problem));
    }
    let settings = loaded.settings;
    Ok(FormatSettingsView {
        apps: settings
            .app_outputs
            .iter()
            .map(|entry| AppFormat {
                id: crate::writings::app_key(&entry.app),
                app: ibis_core::destination::lookup(&entry.app)
                    .map_or_else(|| entry.app.normalized(), |app| app.name.to_owned()),
                format: entry.output.clone(),
            })
            .collect(),
        writings: settings.writings.iter().map(crate::writings::saved_view).collect(),
        key_output_visible: settings.key_formats_used
            || settings.key_output.is_some()
            || settings
                .suggestion_reports
                .contains(&ibis_engine::format::FormatKind::Keys)
            || settings.app_outputs.iter().any(|entry| {
                ibis_engine::format::choose(&settings, Some(&entry.app)).kind == ibis_engine::format::FormatKind::Keys
            }),
        unknown: (!settings.unknown.is_empty()).then_some(settings.unknown),
    })
}

#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) fn forget_app_format(app: AppHandle, id: String) -> Result<(), CommandError> {
    app.state::<SettingsFile>().update(|settings| {
        settings
            .app_outputs
            .retain(|entry| crate::writings::app_key(&entry.app) != id);
    })?;
    let _ = tauri::Emitter::emit(&app, "writings:changed", ());
    Ok(())
}
