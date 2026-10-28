//! Changing the shortcut that opens Ibis, from Settings › Shortcuts (#776).

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{Modifiers, Shortcut};

use crate::diagnostics::{NativeIssue, report};
use crate::picker::{SHORTCUT_BACKEND_UNAVAILABLE, ShortcutStatus, default_shortcut, replace_shortcut, shortcut_label};
use crate::preferences::SettingsFile;

/// Serialize registration, publication and saving, including across window closes.
#[derive(Default)]
pub(crate) struct Changing(tauri::async_runtime::Mutex<()>);

/// Why a shortcut change failed. Only NotSaved changes the working shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ShortcutRefusal {
    /// Not a key combination Ibis can register.
    Invalid,
    /// Shift alone, or no modifier: typing would open Ibis.
    NeedsModifier,
    /// ⌘ (Ctrl elsewhere) with one key is what apps use for their own commands.
    AppShortcut,
    /// Another app or the system already has it.
    InUse,
    /// This desktop gives apps no global shortcuts (Wayland, no X11).
    Unavailable,
    /// It works now, but could not be saved for the next start.
    NotSaved,
}

/// Whether `text` can open Ibis. It needs Control, Option/Alt or Command, and
/// not the command key alone, which every app uses for its own menu commands.
pub(crate) fn check_shortcut(text: &str) -> Result<Shortcut, ShortcutRefusal> {
    let shortcut: Shortcut = text.parse().map_err(|_| ShortcutRefusal::Invalid)?;
    if !shortcut
        .mods
        .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER)
    {
        return Err(ShortcutRefusal::NeedsModifier);
    }
    let command = if cfg!(target_os = "macos") {
        Modifiers::SUPER
    } else {
        Modifiers::CONTROL
    };
    if shortcut.mods == command {
        return Err(ShortcutRefusal::AppShortcut);
    }
    Ok(shortcut)
}

/// Open Ibis with `shortcut` from now on (`None`: the default), and remember it.
/// Every window and the menu bar item show the change at once.
#[tauri::command]
#[specta::specta]
pub(crate) async fn set_shortcut(
    app: AppHandle,
    settings_file: State<'_, SettingsFile>,
    changing: State<'_, Changing>,
    shortcut: Option<String>,
) -> Result<ShortcutStatus, ShortcutRefusal> {
    let requested = match shortcut.as_deref() {
        Some(text) => check_shortcut(text)?,
        None => default_shortcut(),
    };
    let _changing = changing.0.lock().await;
    let (sender, receiver) = std::sync::mpsc::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = sender.send(replace_shortcut(&handle, requested));
    })
    .map_err(|_| ShortcutRefusal::InUse)?;
    let note = tauri::async_runtime::spawn_blocking(move || receiver.recv())
        .await
        .ok()
        .and_then(Result::ok)
        .ok_or(ShortcutRefusal::InUse)?
        .map_err(|error| {
            if error == SHORTCUT_BACKEND_UNAVAILABLE {
                ShortcutRefusal::Unavailable
            } else {
                ShortcutRefusal::InUse
            }
        })?;

    let status = ShortcutStatus {
        shortcut: shortcut.clone().unwrap_or_else(|| shortcut_label().to_string()),
        registered: true,
        error: note.map(str::to_string),
    };
    if let Ok(mut current) = app.state::<Mutex<ShortcutStatus>>().lock() {
        *current = status.clone();
    }
    report(app.emit("shortcut:status", status.clone()), NativeIssue::Shortcut);
    crate::tray::show_shortcut(&app, Some(&status.shortcut));

    let settings_file = settings_file.inner().clone();
    tauri::async_runtime::spawn_blocking(move || save_shortcut(&settings_file, shortcut))
        .await
        .ok()
        .and_then(Result::ok)
        .ok_or(ShortcutRefusal::NotSaved)?;
    Ok(status)
}

fn save_shortcut(file: &SettingsFile, shortcut: Option<String>) -> Result<(), ShortcutRefusal> {
    file.update(|settings| settings.shortcut = shortcut)
        .map(|_| ())
        .map_err(|_| ShortcutRefusal::NotSaved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_and_resetting_the_shortcut_keeps_other_settings() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("fixture failed: {error}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        file.update(|settings| settings.paste = Some(false))
            .unwrap_or_else(|error| panic!("fixture failed: {error}"));
        assert_eq!(save_shortcut(&file, Some("Alt+K".into())), Ok(()));
        let saved = file
            .update(|_| {})
            .unwrap_or_else(|error| panic!("fixture failed: {error}"));
        assert_eq!(saved.shortcut.as_deref(), Some("Alt+K"));
        assert_eq!(saved.paste, Some(false));
        assert_eq!(save_shortcut(&file, None), Ok(()));
        let saved = file
            .update(|_| {})
            .unwrap_or_else(|error| panic!("fixture failed: {error}"));
        assert_eq!(saved.shortcut, None);
        assert_eq!(saved.paste, Some(false));
    }

    #[test]
    fn a_failed_write_returns_the_not_saved_refusal() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("fixture failed: {error}"));
        let path = directory.path().join("settings.json");
        std::fs::create_dir(&path).unwrap_or_else(|error| panic!("fixture failed: {error}"));
        assert_eq!(
            save_shortcut(&SettingsFile::new(path), Some("Alt+K".into())),
            Err(ShortcutRefusal::NotSaved)
        );
    }

    #[test]
    fn a_shortcut_needs_a_real_modifier_and_not_the_command_key_alone() {
        assert_eq!(check_shortcut("Nonsense+Y").err(), Some(ShortcutRefusal::Invalid));
        assert_eq!(check_shortcut("Y").err(), Some(ShortcutRefusal::NeedsModifier));
        assert_eq!(check_shortcut("Shift+Y").err(), Some(ShortcutRefusal::NeedsModifier));
        assert_eq!(
            check_shortcut("CommandOrControl+R").err(),
            Some(ShortcutRefusal::AppShortcut)
        );
        assert_eq!(
            check_shortcut("CommandOrControl+Space").err(),
            Some(ShortcutRefusal::AppShortcut)
        );
        for good in [
            "Control+Command+R",
            "Alt+Space",
            "CommandOrControl+Shift+Y",
            "Control+Alt+C",
        ] {
            assert!(check_shortcut(good).is_ok(), "{good}");
        }
    }

    #[test]
    fn refusals_have_the_names_the_interface_reads() {
        for (refusal, name) in [
            (ShortcutRefusal::Invalid, "invalid"),
            (ShortcutRefusal::NeedsModifier, "needs-modifier"),
            (ShortcutRefusal::AppShortcut, "app-shortcut"),
            (ShortcutRefusal::InUse, "in-use"),
            (ShortcutRefusal::Unavailable, "unavailable"),
            (ShortcutRefusal::NotSaved, "not-saved"),
        ] {
            assert_eq!(serde_json::to_value(refusal).ok(), Some(serde_json::json!(name)));
        }
    }
}
