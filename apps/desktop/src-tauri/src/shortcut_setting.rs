//! Changing the shortcut that opens Ibis, from Settings › Shortcuts (#776).

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

use crate::diagnostics::{NativeIssue, report};
use crate::picker::{
    OpeningShortcut, SHORTCUT_BACKEND_UNAVAILABLE, ShortcutStatus, default_shortcut, replace_shortcut, shortcut_label,
};
use crate::preferences::SettingsFile;

/// Serialize registration, publication and saving, including across window closes.
#[derive(Default)]
pub(crate) struct Changing(tauri::async_runtime::Mutex<()>);

/// Command identifiers shared with KEY_TABLE and the refusal's translated name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PickerCommand {
    Format,
    Pin,
    Keys,
    More,
    Preview,
    Settings,
    Quit,
}

impl std::fmt::Display for PickerCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Format => "Choose Output Format",
            Self::Pin => "Keep Window Open",
            Self::Keys => "Keyboard Shortcuts",
            Self::More => "More",
            Self::Preview => "Show What Will Be Pasted",
            Self::Settings => "Settings",
            Self::Quit => "Quit Ibis",
        })
    }
}

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
    /// The picker already uses this key combination for a command.
    PickerShortcut(PickerCommand),
    /// Windows uses Ctrl+Alt (also with Shift) to type with AltGr.
    AltGr,
    /// Another app or the system already has it.
    InUse,
    /// This desktop gives apps no global shortcuts (Wayland, no X11).
    Unavailable,
    /// It works now, but could not be saved for the next start.
    NotSaved,
}

impl std::fmt::Display for ShortcutRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Invalid => "Ibis cannot use that key combination",
            Self::NeedsModifier => "Include Control, Alt or Command; Shift alone is not enough",
            Self::AppShortcut => "That shortcut is used for app commands; add another modifier",
            Self::PickerShortcut(command) => return write!(f, "That shortcut is used for {command}"),
            Self::AltGr => "Ctrl+Alt with a typing key is used for AltGr on Windows",
            Self::InUse => "Another app or the system may be using that shortcut",
            Self::Unavailable => "Global shortcuts are unavailable on this desktop",
            Self::NotSaved => "The shortcut works now, but Ibis could not save it for next time",
        })
    }
}

/// Whether `text` can open Ibis. It needs Control, Option/Alt or Command, and
/// not the command key alone or a chord already used by the picker.
pub(crate) fn check_shortcut(text: &str) -> Result<OpeningShortcut, ShortcutRefusal> {
    let requested = text.parse().map_err(|_| ShortcutRefusal::Invalid)?;
    validate_shortcut(requested)
}

/// Native layout queries run on the main thread, alongside registration.
pub(crate) fn validate_shortcut(requested: OpeningShortcut) -> Result<OpeningShortcut, ShortcutRefusal> {
    let command = if cfg!(target_os = "macos") {
        Modifiers::SUPER
    } else {
        Modifiers::CONTROL
    };
    #[cfg(target_os = "macos")]
    let typed = crate::picker::typing();
    validate_shortcut_for(requested, command, cfg!(target_os = "windows"), {
        #[cfg(target_os = "macos")]
        {
            Some(&typed)
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    })
}

#[cfg(test)]
pub(crate) fn check_shortcut_for(
    text: &str,
    command: Modifiers,
    windows: bool,
) -> Result<OpeningShortcut, ShortcutRefusal> {
    check_shortcut_with_layout(text, command, windows, None)
}

#[cfg(test)]
fn check_shortcut_with_layout(
    text: &str,
    command: Modifiers,
    windows: bool,
    typed: Option<&dyn Fn(u16, Modifiers) -> Option<char>>,
) -> Result<OpeningShortcut, ShortcutRefusal> {
    let requested: OpeningShortcut = text.parse().map_err(|_| ShortcutRefusal::Invalid)?;
    validate_shortcut_for(requested, command, windows, typed)
}

fn validate_shortcut_for(
    requested: OpeningShortcut,
    command: Modifiers,
    windows: bool,
    typed: Option<&dyn Fn(u16, Modifiers) -> Option<char>>,
) -> Result<OpeningShortcut, ShortcutRefusal> {
    let shortcut = requested.shortcut;
    if !shortcut
        .mods
        .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER)
    {
        return Err(ShortcutRefusal::NeedsModifier);
    }
    if shortcut.mods == command || shortcut.mods == command | Modifiers::SHIFT {
        // Resolve a logical letter just as registration does; physical recorder
        // codes stay put. Query the resulting event with Shift to recognize ?.
        let effective = typed.map_or(shortcut, |typed| crate::picker::on_layout_with(requested, typed).0);
        let character = typed
            .and_then(|typed| crate::picker::character(effective, typed))
            .or_else(|| crate::picker::us_character(effective));
        if let Some(command) = picker_command(effective, character) {
            return Err(ShortcutRefusal::PickerShortcut(command));
        }
        if shortcut.mods == command {
            return Err(ShortcutRefusal::AppShortcut);
        }
    }
    if windows && uses_altgr(requested) {
        return Err(ShortcutRefusal::AltGr);
    }
    Ok(requested)
}

/// Match KEY_TABLE and picker/shortcuts.ts, including their non-Latin letter
/// fallback and the shifted punctuation accepted by isKeysShortcut.
fn picker_command(shortcut: Shortcut, character: Option<char>) -> Option<PickerCommand> {
    let letter = |letter: char, code: Code| {
        character.is_some_and(|typed| {
            typed.eq_ignore_ascii_case(&letter)
                || (shortcut.key == code && (!typed.is_ascii() || (code == Code::KeyQ && matches!(typed, ';' | '/'))))
        })
    };
    if shortcut.mods.contains(Modifiers::SHIFT) {
        if letter('f', Code::KeyF) {
            return Some(PickerCommand::Format);
        }
        if letter('p', Code::KeyP) {
            return Some(PickerCommand::Pin);
        }
    } else {
        if letter('k', Code::KeyK) {
            return Some(PickerCommand::More);
        }
        if letter('q', Code::KeyQ) {
            return Some(PickerCommand::Quit);
        }
        if character.is_some_and(|typed| typed.eq_ignore_ascii_case(&'i')) {
            return Some(PickerCommand::Preview);
        }
        if character == Some(',') {
            return Some(PickerCommand::Settings);
        }
    }
    matches!(character, Some('/' | '?')).then_some(PickerCommand::Keys)
}

fn uses_altgr(requested: OpeningShortcut) -> bool {
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
    if crate::picker::desktop_owned() {
        return Err(ShortcutRefusal::Unavailable);
    }
    let _changing = changing.0.lock().await;
    let (sender, receiver) = std::sync::mpsc::channel();
    let handle = app.clone();
    let change = shortcut.clone();
    app.run_on_main_thread(move || {
        let result = change
            .as_deref()
            .map_or_else(|| Ok(default_shortcut()), check_shortcut)
            .and_then(|requested| {
                replace_shortcut(&handle, requested).map_err(|error| {
                    if error == SHORTCUT_BACKEND_UNAVAILABLE {
                        ShortcutRefusal::Unavailable
                    } else {
                        ShortcutRefusal::InUse
                    }
                })
            });
        let _ = sender.send(result);
    })
    .map_err(|_| ShortcutRefusal::InUse)?;
    let note = tauri::async_runtime::spawn_blocking(move || receiver.recv())
        .await
        .ok()
        .and_then(Result::ok)
        .ok_or(ShortcutRefusal::InUse)??;

    let status = ShortcutStatus {
        shortcut: shortcut.clone().unwrap_or_else(|| shortcut_label().to_string()),
        registered: true,
        error: note.map(str::to_string),
        desktop: None,
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
    fn azerty_help_shortcut_conflicts_use_the_recorded_physical_key() {
        let azerty = |key, modifiers: Modifiers| match (key, modifiers.contains(Modifiers::SHIFT)) {
            (46, false) => Some(','),
            (46, true) => Some('?'),
            (41, false) => Some('m'),
            (41, true) => Some('M'),
            _ => None,
        };
        assert_eq!(
            check_shortcut_with_layout("Shift+Command+KeyM", Modifiers::SUPER, false, Some(&azerty)).err(),
            Some(ShortcutRefusal::PickerShortcut(PickerCommand::Keys))
        );
        let logical = check_shortcut_with_layout("Shift+Command+M", Modifiers::SUPER, false, Some(&azerty))
            .unwrap_or_else(|error| panic!("logical M should remain available: {error}"));
        assert_eq!(crate::picker::on_layout_with(logical, azerty).0.key, Code::Semicolon);
        assert!(check_shortcut_with_layout("Shift+Command+KeyM", Modifiers::SUPER, false, None).is_ok());
        // A physical F types U on Dvorak, while logical F moves to physical Y.
        let dvorak = |key, _: Modifiers| match key {
            3 => Some('u'),
            16 => Some('f'),
            _ => None,
        };
        assert!(check_shortcut_with_layout("Shift+Command+KeyF", Modifiers::SUPER, false, Some(&dvorak)).is_ok());
        assert_eq!(
            check_shortcut_with_layout("Shift+Command+F", Modifiers::SUPER, false, Some(&dvorak)).err(),
            Some(ShortcutRefusal::PickerShortcut(PickerCommand::Format))
        );
    }

    #[test]
    fn recorded_picker_shortcut_conflicts_name_the_owning_command() {
        #[derive(serde::Deserialize)]
        struct Conflict {
            name: String,
            shortcut: String,
            command: String,
            label: String,
            typed: std::collections::BTreeMap<u16, [char; 2]>,
        }
        let conflicts: Vec<Conflict> =
            serde_json::from_str(include_str!("../../src/testing/picker-shortcut-conflicts.json"))
                .unwrap_or_else(|error| panic!("invalid picker conflicts: {error}"));
        for case in conflicts {
            let typed = |key, modifiers: Modifiers| {
                case.typed
                    .get(&key)
                    .map(|letters| letters[usize::from(modifiers.contains(Modifiers::SHIFT))])
            };
            let validate = |text: &str| check_shortcut_with_layout(text, Modifiers::SUPER, false, Some(&typed));
            let refusal = validate(&case.shortcut).err();
            assert_eq!(
                refusal.and_then(|refusal| serde_json::to_value(refusal).ok()),
                Some(serde_json::json!({"picker-shortcut": case.command})),
                "{}",
                case.name
            );
            // Previously saved conflicts take the same path after upgrading.
            let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("fixture failed: {error}"));
            let file = SettingsFile::new(directory.path().join("settings.json"));
            assert_eq!(save_shortcut(&file, Some(case.shortcut)), Ok(()));
            let saved = ibis_core::settings::load(&file.path);
            let (shortcut, label, error) =
                crate::picker::configured_shortcut_with(saved.settings.shortcut.as_deref(), validate);
            assert_eq!(shortcut, default_shortcut(), "{}", case.name);
            assert_eq!(label, shortcut_label());
            assert!(error.is_some_and(|error| error.contains(&case.label)), "{}", case.name);
        }
    }

    #[test]
    fn opening_shortcuts_cannot_take_picker_commands_on_any_platform() {
        // keys.test.ts keeps these cases in sync with KEY_TABLE and the
        // shifted keys-help alias accepted by the picker key handler.
        let commands: Vec<String> =
            serde_json::from_str(include_str!("../../src/testing/picker-command-shortcuts.json"))
                .unwrap_or_else(|error| panic!("invalid picker shortcuts: {error}"));
        for (command, windows, modifier) in [
            (Modifiers::SUPER, false, "Command"),
            (Modifiers::CONTROL, true, "Control"),
            (Modifiers::CONTROL, false, "Control"),
        ] {
            for keys in &commands {
                let text = keys.replace("CmdOrCtrl", modifier);
                assert!(check_shortcut_for(&text, command, windows).is_err(), "{text}");
            }
            for key in ["F", "KeyF", "P", "KeyP", "Slash", "/"] {
                let text = format!("Shift+{modifier}+{key}");
                assert_eq!(
                    check_shortcut_for(&text, command, windows).err(),
                    Some(ShortcutRefusal::PickerShortcut(match key {
                        "F" | "KeyF" => PickerCommand::Format,
                        "P" | "KeyP" => PickerCommand::Pin,
                        _ => PickerCommand::Keys,
                    })),
                    "{text}"
                );
            }
            for key in ["Y", "K", "I", "Q", "Comma"] {
                let text = format!("{modifier}+Shift+{key}");
                assert!(check_shortcut_for(&text, command, windows).is_ok(), "{text}");
            }
            for keys in [
                "Alt+Shift+F",
                "Alt+Shift+P",
                "Alt+Shift+Slash",
                "Control+Command+Shift+F",
            ] {
                assert!(check_shortcut_for(keys, command, windows).is_ok(), "{keys}");
            }
        }
    }

    #[derive(serde::Deserialize)]
    struct Recording {
        name: String,
        shortcut: String,
        typed: std::collections::BTreeMap<u16, char>,
        registered: String,
        windows_refusal: Option<serde_json::Value>,
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
                crate::picker::configured_shortcut_with(saved.settings.shortcut.as_deref(), |text| {
                    check_shortcut_for(text, Modifiers::SUPER, false)
                });
            assert_eq!(error, None, "{}", case.name);
            assert_eq!(label, case.shortcut);
            assert_eq!(restarted, requested, "{}", case.name);
            if case.windows_refusal.is_some() {
                let (safe, label, error) =
                    crate::picker::configured_shortcut_for(saved.settings.shortcut.as_deref(), true);
                assert_eq!(safe, default_shortcut(), "{}", case.name);
                assert_eq!(label, shortcut_label());
                assert!(error.is_some());
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
            assert_eq!(refusal, case.windows_refusal, "{}", case.name);
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
        let check = |text| {
            check_shortcut_for(
                text,
                if cfg!(target_os = "macos") {
                    Modifiers::SUPER
                } else {
                    Modifiers::CONTROL
                },
                cfg!(target_os = "windows"),
            )
        };
        assert_eq!(check("Nonsense+Y").err(), Some(ShortcutRefusal::Invalid));
        assert_eq!(check("Y").err(), Some(ShortcutRefusal::NeedsModifier));
        assert_eq!(check("Shift+Y").err(), Some(ShortcutRefusal::NeedsModifier));
        assert_eq!(check("CommandOrControl+R").err(), Some(ShortcutRefusal::AppShortcut));
        assert_eq!(
            check("CommandOrControl+Space").err(),
            Some(ShortcutRefusal::AppShortcut)
        );
        for good in ["Control+Command+R", "Alt+Space", "CommandOrControl+Shift+Y"] {
            assert!(check(good).is_ok(), "{good}");
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
