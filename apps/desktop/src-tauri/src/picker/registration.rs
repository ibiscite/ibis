//! Registering the global shortcut, and moving it with the keyboard layout.

use std::sync::Mutex;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "macos")]
use tauri::Emitter;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use super::OpeningShortcut;
#[cfg(any(target_os = "macos", test))]
use super::ShortcutStatus;
#[cfg(target_os = "macos")]
use super::layout;
#[cfg(target_os = "macos")]
use crate::diagnostics::{NativeIssue, report};

pub(crate) const SHORTCUT_BACKEND_UNAVAILABLE: &str =
    "Global shortcuts need a working X11 connection with XKB. Open Ibis from your application launcher.";
const SHORTCUT_REGISTRATION_FAILED: &str =
    "The shortcut could not be registered. Another app may be using it; choose a different shortcut in settings.";
const NO_KEY_TYPES_THE_LETTER: &str =
    "No key on this keyboard layout types the shortcut's letter, so it uses that letter's place on a US keyboard.";

/// The shortcut from settings and the key registered for it on the active layout.
#[derive(Default)]
pub(crate) struct Registered(Mutex<Option<(OpeningShortcut, Shortcut)>>);

/// The key to register for `shortcut` on the active keyboard layout, and a
/// note when no key types its letter. Only macOS registers physical keys.
fn on_layout(requested: OpeningShortcut) -> (Shortcut, Option<&'static str>) {
    #[cfg(target_os = "macos")]
    return on_layout_with(requested, layout::typing());
    #[cfg(not(target_os = "macos"))]
    (requested.shortcut, None)
}

pub(crate) fn on_layout_with(
    requested: OpeningShortcut,
    typed: impl Fn(u16, tauri_plugin_global_shortcut::Modifiers) -> Option<char>,
) -> (Shortcut, Option<&'static str>) {
    if !requested.logical_letter {
        return (requested.shortcut, None);
    }
    super::layout::translate(requested.shortcut, typed)
        .map_or((requested.shortcut, Some(NO_KEY_TYPES_THE_LETTER)), |key| (key, None))
}

/// The Linux plugin can return success after its X11 worker exits. Confirm its
/// required backend first; session labels alone do not identify usable X11.
/// Success can carry a note for the person, such as a missing layout letter.
pub(crate) fn register_shortcut(
    app: &AppHandle,
    shortcut: OpeningShortcut,
) -> Result<Option<&'static str>, &'static str> {
    let (key, note) = on_layout(shortcut);
    register_with(
        || {
            #[cfg(target_os = "linux")]
            return super::x11_focus::shortcut_ready();
            #[cfg(not(target_os = "linux"))]
            Ok(())
        },
        || app.global_shortcut().register(key),
    )?;
    if let Ok(mut registered) = app.state::<Registered>().0.lock() {
        *registered = Some((shortcut, key));
    }
    Ok(note)
}

/// Swap the registered shortcut for `requested`, from Settings. The new key is
/// registered before the working one is released, so a refusal keeps it.
/// Runs on the main thread, where layout queries are answered.
pub(crate) fn replace_shortcut(
    app: &AppHandle,
    requested: OpeningShortcut,
) -> Result<Option<&'static str>, &'static str> {
    let state = app.state::<Registered>();
    let mut registered = state.0.lock().map_err(|_| SHORTCUT_REGISTRATION_FAILED)?;
    let (key, note) = on_layout(requested);
    let current = registered.map(|(_, key)| key);
    if current != Some(key) {
        let shortcuts = app.global_shortcut();
        replace_with(
            current,
            key,
            |key| {
                register_with(
                    || {
                        #[cfg(target_os = "linux")]
                        return super::x11_focus::shortcut_ready();
                        #[cfg(not(target_os = "linux"))]
                        Ok(())
                    },
                    || shortcuts.register(key),
                )
            },
            |key| shortcuts.unregister(key).map_err(|_| SHORTCUT_REGISTRATION_FAILED),
        )?;
    }
    *registered = Some((requested, key));
    Ok(note)
}

/// A failed registration keeps the old key; a failed release rolls back the new one.
pub(crate) fn replace_with(
    current: Option<Shortcut>,
    key: Shortcut,
    register: impl FnOnce(Shortcut) -> Result<(), &'static str>,
    mut unregister: impl FnMut(Shortcut) -> Result<(), &'static str>,
) -> Result<(), &'static str> {
    register(key)?;
    if let Some(old) = current
        && let Err(error) = unregister(old)
    {
        let _ = unregister(key);
        return Err(error);
    }
    Ok(())
}

/// Move the shortcut when the person switches keyboard layout. objc2's
/// notification observers need unsafe code, so check every two seconds, with
/// at most one pending main-thread check (layout queries need the main thread).
#[cfg(target_os = "macos")]
pub(crate) fn follow_layout(app: &AppHandle) {
    let handle = app.clone();
    let pending = std::sync::Arc::new(AtomicBool::new(false));
    let watcher = std::thread::Builder::new()
        .name("keyboard-layout".to_owned())
        .spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));
                if pending.swap(true, Ordering::Relaxed) {
                    continue;
                }
                let (task, done) = (handle.clone(), pending.clone());
                if handle
                    .run_on_main_thread(move || {
                        relayout(&task);
                        done.store(false, Ordering::Relaxed);
                    })
                    .is_err()
                {
                    pending.store(false, Ordering::Relaxed);
                }
            }
        });
    report(watcher, NativeIssue::Shortcut);
}

#[cfg(target_os = "macos")]
fn relayout(app: &AppHandle) {
    let state = app.state::<Registered>();
    let Ok(mut registered) = state.0.lock() else {
        return;
    };
    let Some((requested, current)) = *registered else {
        return;
    };
    let (key, note) = on_layout(requested);
    // Register the new key before releasing the working one: on a conflict the
    // working key stays and the next check tries again.
    let moved = if key == current {
        Ok(())
    } else {
        let shortcuts = app.global_shortcut();
        shortcuts
            .register(key)
            .map_err(|_| SHORTCUT_REGISTRATION_FAILED)
            .map(|()| {
                let _ = shortcuts.unregister(current);
                *registered = Some((requested, key));
            })
    };
    let changed = app.state::<Mutex<ShortcutStatus>>().lock().ok().and_then(|mut status| {
        let before = status.error.clone();
        layout_status(&mut status, moved, note);
        (status.error != before).then(|| status.clone())
    });
    // An open picker shows the new note at once, not only when next opened.
    if let Some(status) = changed {
        report(app.emit("shortcut:status", status), NativeIssue::Shortcut);
    }
}

/// Show a layout check's outcome. A settings problem stays visible; only the
/// notes a layout check writes are replaced or cleared.
#[cfg(any(target_os = "macos", test))]
fn layout_status(status: &mut ShortcutStatus, moved: Result<(), &'static str>, note: Option<&'static str>) {
    let ours = [NO_KEY_TYPES_THE_LETTER, SHORTCUT_REGISTRATION_FAILED];
    if status.error.as_deref().is_none_or(|error| ours.contains(&error)) {
        status.error = moved.err().or(note).map(str::to_string);
    }
}

pub(super) fn register_with<E>(
    backend: impl FnOnce() -> Result<(), ()>,
    register: impl FnOnce() -> Result<(), E>,
) -> Result<(), &'static str> {
    backend().map_err(|()| SHORTCUT_BACKEND_UNAVAILABLE)?;
    register().map_err(|_| SHORTCUT_REGISTRATION_FAILED)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_registers_first_and_keeps_the_old_key_on_conflict() {
        use std::cell::RefCell;
        let old = "Alt+Y"
            .parse::<Shortcut>()
            .unwrap_or_else(|error| panic!("fixture failed: {error}"));
        let new = "Alt+K"
            .parse::<Shortcut>()
            .unwrap_or_else(|error| panic!("fixture failed: {error}"));
        for registration in [
            Ok(()),
            Err(SHORTCUT_REGISTRATION_FAILED),
            Err(SHORTCUT_BACKEND_UNAVAILABLE),
        ] {
            let operations = RefCell::new(Vec::new());
            let result = replace_with(
                Some(old),
                new,
                |key| {
                    operations.borrow_mut().push(("register", key));
                    registration
                },
                |key| {
                    operations.borrow_mut().push(("unregister", key));
                    Ok(())
                },
            );
            assert_eq!(result, registration);
            let expected = if registration.is_ok() {
                vec![("register", new), ("unregister", old)]
            } else {
                vec![("register", new)]
            };
            assert_eq!(*operations.borrow(), expected);
        }
    }

    #[test]
    fn replacement_rolls_back_if_the_old_key_cannot_be_released() {
        let old = "Alt+Y"
            .parse::<Shortcut>()
            .unwrap_or_else(|error| panic!("fixture failed: {error}"));
        let new = "Alt+K"
            .parse::<Shortcut>()
            .unwrap_or_else(|error| panic!("fixture failed: {error}"));
        let mut released = Vec::new();
        let result = replace_with(
            Some(old),
            new,
            |_| Ok(()),
            |key| {
                released.push(key);
                if key == old {
                    Err(SHORTCUT_REGISTRATION_FAILED)
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(result, Err(SHORTCUT_REGISTRATION_FAILED));
        assert_eq!(released, [old, new]);
        assert!(replace_with(None, new, |_| Ok(()), |_| panic!("no previous shortcut")).is_ok());
    }

    #[test]
    fn unavailable_shortcut_backend_never_calls_the_registration_plugin() {
        assert_eq!(
            register_with::<()>(
                || Err(()),
                || panic!("a disconnected plugin can falsely return success")
            ),
            Err(SHORTCUT_BACKEND_UNAVAILABLE)
        );
        assert_eq!(register_with(|| Ok(()), || Ok::<(), ()>(())), Ok(()));
    }

    #[test]
    fn shortcut_registration_conflicts_remain_visible_without_raw_errors() {
        struct PrivateError;
        impl std::fmt::Display for PrivateError {
            fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                panic!("never format a native error");
            }
        }
        assert_eq!(
            register_with(|| Ok(()), || Err(PrivateError)),
            Err(SHORTCUT_REGISTRATION_FAILED)
        );
    }

    #[test]
    fn a_layout_check_updates_its_own_notes_and_keeps_settings_problems() {
        let mut status = ShortcutStatus {
            shortcut: "Command+Shift+Y".into(),
            registered: true,
            error: None,
            desktop: None,
        };
        layout_status(&mut status, Ok(()), Some(NO_KEY_TYPES_THE_LETTER));
        assert_eq!(status.error.as_deref(), Some(NO_KEY_TYPES_THE_LETTER));
        layout_status(&mut status, Err(SHORTCUT_REGISTRATION_FAILED), None);
        assert_eq!(status.error.as_deref(), Some(SHORTCUT_REGISTRATION_FAILED));
        assert!(status.registered, "the working key stays registered");
        layout_status(&mut status, Ok(()), None);
        assert_eq!(status.error, None);
        status.error = Some("the shortcut in settings is not valid".into());
        layout_status(&mut status, Ok(()), Some(NO_KEY_TYPES_THE_LETTER));
        assert_eq!(status.error.as_deref(), Some("the shortcut in settings is not valid"));
    }
}
