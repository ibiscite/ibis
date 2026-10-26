//! Registering the global shortcut, and moving it with the keyboard layout.

use std::sync::Mutex;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "macos")]
use tauri::Emitter;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

#[cfg(any(target_os = "macos", test))]
use super::ShortcutStatus;
#[cfg(target_os = "macos")]
use super::layout;
#[cfg(target_os = "macos")]
use crate::diagnostics::{NativeIssue, report};

pub(super) const SHORTCUT_BACKEND_UNAVAILABLE: &str =
    "Global shortcuts need a working X11 connection with XKB. Open Ibis from your application launcher.";
const SHORTCUT_REGISTRATION_FAILED: &str =
    "The shortcut could not be registered. Another app may be using it; choose a different shortcut in settings.";
#[cfg(any(target_os = "macos", test))]
const NO_KEY_TYPES_THE_LETTER: &str =
    "No key on this keyboard layout types the shortcut's letter, so it uses that letter's place on a US keyboard.";

/// The shortcut from settings and the key registered for it on the active layout.
#[derive(Default)]
pub(crate) struct Registered(Mutex<Option<(Shortcut, Shortcut)>>);

/// The key to register for `shortcut` on the active keyboard layout, and a
/// note when no key types its letter. Only macOS registers physical keys.
fn on_layout(shortcut: Shortcut) -> (Shortcut, Option<&'static str>) {
    #[cfg(target_os = "macos")]
    return layout::translate(shortcut, layout::typing())
        .map_or((shortcut, Some(NO_KEY_TYPES_THE_LETTER)), |key| (key, None));
    #[cfg(not(target_os = "macos"))]
    (shortcut, None)
}

/// The Linux plugin can return success after its X11 worker exits. Confirm its
/// required backend first; session labels alone do not identify usable X11.
/// Success can carry a note for the person, such as a missing layout letter.
pub(crate) fn register_shortcut(app: &AppHandle, shortcut: Shortcut) -> Result<Option<&'static str>, &'static str> {
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
