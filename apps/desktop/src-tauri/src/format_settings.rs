//! Settings views contain names and format IDs, never bibliography paths.
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::{library::CommandError, preferences::SettingsFile};

#[derive(Serialize, schemars::JsonSchema)]
pub(crate) struct AppFormat {
    id: String,
    /// Resolved at display time by the same native lookup as the picker.
    app: String,
    format: String,
}

#[derive(Serialize, schemars::JsonSchema)]
pub(crate) struct FormatSettingsView {
    apps: Vec<AppFormat>,
    writings: Vec<ibis_engine::app_context::WritingView>,
    key_output_visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    unknown: Option<serde_json::Map<String, serde_json::Value>>,
}

#[tauri::command]
#[ibis_desktop_macros::command(main)]
pub(crate) fn format_settings(app: AppHandle) -> Result<FormatSettingsView, CommandError> {
    let loaded = app.state::<SettingsFile>().owner.view();
    let settings = loaded.cites().map_err(CommandError::from)?;
    Ok(FormatSettingsView {
        apps: settings
            .device
            .app_outputs
            .iter()
            .map(|entry| AppFormat {
                id: ibis_engine::app_context::app_key(&entry.app),
                app: crate::destination::display_name(&entry.app),
                format: entry.output.clone(),
            })
            .collect(),
        writings: settings
            .device
            .writings
            .iter()
            .map(ibis_engine::app_context::saved_view)
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
        unknown: (!settings.unknown.is_empty()).then(|| settings.unknown.clone()),
    })
}

#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn forget_app_format(app: AppHandle, id: String) -> Result<(), CommandError> {
    app.state::<SettingsFile>().update(|settings| {
        settings
            .device
            .app_outputs
            .retain(|entry| ibis_engine::app_context::app_key(&entry.app) != id);
    })?;
    let _ = tauri::Emitter::emit(&app, "writings:changed", ());
    Ok(())
}
