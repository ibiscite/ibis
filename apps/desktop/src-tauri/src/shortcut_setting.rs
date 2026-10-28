//! Changing the shortcut that opens Ibis, from Settings › Shortcuts (#776).

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{Code, Modifiers};

use crate::diagnostics::{NativeIssue, report};
use crate::picker::{
    OpeningShortcut, SHORTCUT_BACKEND_UNAVAILABLE, ShortcutStatus, default_shortcut, replace_shortcut, shortcut_label,
};
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
    /// Windows uses Ctrl+Alt (also with Shift) to type with AltGr.
    AltGr,
    /// Another app or the system already has it.
    InUse,
    /// This desktop gives apps no global shortcuts (Wayland, no X11).
    Unavailable,
    /// It works now, but could not be saved for the next start.
    NotSaved,
}

/// Whether `text` can open Ibis. It needs Control, Option/Alt or Command, and
/// not the command key alone, which every app uses for its own menu commands.
pub(crate) fn check_shortcut(text: &str) -> Result<OpeningShortcut, ShortcutRefusal> {
    let command = if cfg!(target_os = "macos") {
        Modifiers::SUPER
    } else {
        Modifiers::CONTROL
    };
    check_shortcut_for(text, command, cfg!(target_os = "windows"))
}

fn check_shortcut_for(text: &str, command: Modifiers, windows: bool) -> Result<OpeningShortcut, ShortcutRefusal> {
    let requested: OpeningShortcut = text.parse().map_err(|_| ShortcutRefusal::Invalid)?;
    let shortcut = requested.shortcut;
    if !shortcut
        .mods
        .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER)
    {
        return Err(ShortcutRefusal::NeedsModifier);
    }
    if shortcut.mods == command {
        return Err(ShortcutRefusal::AppShortcut);
    }
    if windows && uses_altgr(requested) {
        return Err(ShortcutRefusal::AltGr);
    }
    Ok(requested)
}

pub(crate) fn uses_altgr(requested: OpeningShortcut) -> bool {
    requested.shortcut.mods.difference(Modifiers::SHIFT) == Modifiers::CONTROL | Modifiers::ALT
        && printable_key(requested.shortcut.key)
}

/// All printable codes the shortcut parser accepts, including keypad keys.
fn printable_key(key: Code) -> bool {
    let name = key.to_string();
    name.starts_with("Key")
        || name.starts_with("Digit")
        || (name.starts_with("Numpad") && key != Code::NumpadEnter)
        || matches!(
            key,
            Code::Backquote
                | Code::Backslash
                | Code::BracketLeft
                | Code::BracketRight
                | Code::Comma
                | Code::Equal
                | Code::Minus
                | Code::Period
                | Code::Quote
                | Code::Semicolon
                | Code::Slash
                | Code::Space
        )
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

    #[derive(serde::Deserialize)]
    struct Recording {
        name: String,
        shortcut: String,
        typed: std::collections::BTreeMap<u16, char>,
        registered: String,
        windows_refusal: Option<String>,
    }

    /// DOM tests send these exact accelerators to Settings. Exercise the same
    /// values through validation, disk, startup parsing and native replacement.
    #[test]
    fn recorded_shortcuts_keep_key_intent_through_save_restart_and_registration() {
        let cases: Vec<Recording> = serde_json::from_str(include_str!("../../src/testing/shortcut-recordings.json"))
            .unwrap_or_else(|error| panic!("invalid shortcut recordings: {error}"));
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("fixture failed: {error}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        for case in cases {
            let requested = check_shortcut_for(&case.shortcut, Modifiers::SUPER, false)
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name));
            assert_eq!(save_shortcut(&file, Some(case.shortcut.clone())), Ok(()));
            let saved = ibis_core::settings::load(&file.path);
            assert!(saved.problem.is_none());
            let (restarted, label, error) =
                crate::picker::configured_shortcut_for(saved.settings.shortcut.as_deref(), false);
            assert_eq!(error, None, "{}", case.name);
            assert_eq!(label, case.shortcut);
            assert_eq!(restarted, requested, "{}", case.name);
            if case.windows_refusal.as_deref() == Some("alt-gr") {
                let (safe, label, error) =
                    crate::picker::configured_shortcut_for(saved.settings.shortcut.as_deref(), true);
                assert_eq!(safe, default_shortcut(), "{}", case.name);
                assert_eq!(label, shortcut_label());
                assert!(error.is_some_and(|error| error.contains("AltGr")));
            }
            for binding in [requested, restarted] {
                let (key, note) = crate::picker::on_layout_with(binding, |key, _| case.typed.get(&key).copied());
                assert_eq!(note, None, "{}", case.name);
                let expected = case
                    .registered
                    .parse::<tauri_plugin_global_shortcut::Shortcut>()
                    .unwrap_or_else(|error| panic!("fixture failed: {error}"));
                assert_eq!(key, expected, "{}", case.name);
                let old = default_shortcut().shortcut;
                let operations = std::cell::RefCell::new(Vec::new());
                assert_eq!(
                    crate::picker::replace_with(
                        Some(old),
                        key,
                        |key| {
                            operations.borrow_mut().push(("register", key));
                            Ok(())
                        },
                        |key| {
                            operations.borrow_mut().push(("unregister", key));
                            Ok(())
                        }
                    ),
                    Ok(())
                );
                assert_eq!(*operations.borrow(), [("register", expected), ("unregister", old)]);
                if !binding.logical_letter {
                    assert_eq!(
                        crate::picker::on_layout_with(binding, |_, _| panic!(
                            "physical keys must not query the layout"
                        )),
                        (expected, None)
                    );
                }
            }
            let refusal = check_shortcut_for(&case.shortcut, Modifiers::CONTROL, true)
                .err()
                .map(|error| serde_json::to_value(error).unwrap_or_else(|error| panic!("fixture failed: {error}")));
            assert_eq!(
                refusal,
                case.windows_refusal.map(serde_json::Value::String),
                "{}",
                case.name
            );
        }
    }

    #[test]
    fn windows_shortcuts_leave_altgr_printing_keys_available() {
        for key in [
            "C",
            "KeyC",
            "Digit1",
            "Quote",
            "BracketRight",
            "Space",
            "Numpad0",
            "NumpadAdd",
            "NumpadDecimal",
        ] {
            for modifiers in ["Control+Alt", "Control+Alt+Shift"] {
                let text = format!("{modifiers}+{key}");
                assert_eq!(
                    check_shortcut_for(&text, Modifiers::CONTROL, true).err(),
                    Some(ShortcutRefusal::AltGr),
                    "{text}"
                );
                assert!(check_shortcut_for(&text, Modifiers::SUPER, false).is_ok());
                assert!(check_shortcut_for(&text, Modifiers::CONTROL, false).is_ok());
            }
        }
        for text in [
            "Control+Alt+F8",
            "Control+Alt+Shift+ArrowLeft",
            "Control+Alt+Delete",
            "Control+Alt+NumpadEnter",
            "Control+Alt+Command+KeyC",
            "Control+Shift+C",
            "Alt+KeyC",
        ] {
            assert!(check_shortcut_for(text, Modifiers::CONTROL, true).is_ok(), "{text}");
        }
    }

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
        for good in ["Control+Command+R", "Alt+Space", "CommandOrControl+Shift+Y"] {
            assert!(check_shortcut(good).is_ok(), "{good}");
        }
    }

    #[test]
    fn refusals_have_the_names_the_interface_reads() {
        for (refusal, name) in [
            (ShortcutRefusal::Invalid, "invalid"),
            (ShortcutRefusal::NeedsModifier, "needs-modifier"),
            (ShortcutRefusal::AppShortcut, "app-shortcut"),
            (ShortcutRefusal::AltGr, "alt-gr"),
            (ShortcutRefusal::InUse, "in-use"),
            (ShortcutRefusal::Unavailable, "unavailable"),
            (ShortcutRefusal::NotSaved, "not-saved"),
        ] {
            assert_eq!(serde_json::to_value(refusal).ok(), Some(serde_json::json!(name)));
        }
    }
}
