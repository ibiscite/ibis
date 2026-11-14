//! Settings views contain names and format IDs, never bibliography paths.
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::preferences::SettingsFile;

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
}

#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) fn format_settings(app: AppHandle) -> Result<FormatSettingsView, String> {
    let loaded = ibis_engine::settings::load(&app.state::<SettingsFile>().path);
    let settings = loaded.cites().map_err(ibis_engine::settings_backup::settings_message)?;
    Ok(FormatSettingsView {
        apps: settings
            .device
            .app_outputs
            .iter()
            .map(|entry| AppFormat {
                id: crate::writings::app_key(&entry.app),
                app: ibis_core::destination::lookup(&entry.app)
                    .map_or_else(|| entry.app.normalized(), |app| app.name.to_owned()),
                format: entry.output.clone(),
            })
            .collect(),
        writings: settings
            .device
            .writings
            .iter()
            .map(crate::writings::saved_view)
            .collect(),
        key_output_visible: settings.device.key_formats_used
            || settings.portable.key_output.is_some()
            || settings
                .device
                .suggestion_reports
                .contains(&ibis_engine::format::FormatKind::Keys)
            || settings.device.app_outputs.iter().any(|entry| {
                ibis_engine::format::choose(settings, Some(&entry.app)).kind == ibis_engine::format::FormatKind::Keys
            }),
    })
}

#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) fn forget_app_format(app: AppHandle, id: String) -> Result<(), String> {
    app.state::<SettingsFile>().update(|settings| {
        settings
            .device
            .app_outputs
            .retain(|entry| crate::writings::app_key(&entry.app) != id);
    })?;
    let _ = tauri::Emitter::emit(&app, "writings:changed", ());
    Ok(())
}
