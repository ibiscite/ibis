//! Enter pastes into the app the picker was opened from (#27, #1241).
//!
//! The capture, its proofs and the native calls live in `ibis_native::paste`
//! (decision 0029). This module binds them to the app: its windows, settings,
//! per-app choices, keyboard layout and clipboard write tokens.

use std::ops::Deref;

use ibis_core::destination::{AppId, AppPaste, Decision, PastePermission, lookup};
#[cfg(target_os = "windows")]
use ibis_core::destination::{CopyReason, System};
#[cfg(test)]
use ibis_native::paste::queries;
#[cfg(any(target_os = "windows", test))]
use ibis_native::paste::queries::QueryWorker;
pub(crate) use ibis_native::paste::{Captured, CopiedReason, CopyAttempt, Delivered, capture};
#[cfg(target_os = "windows")]
use ibis_native::paste::{
    DeliveryTicket, key_release_failed,
    keys::{PasteEvent, PasteKeys},
    queries::{PasswordQueries, display_name},
    tickets::{TicketDesk, deliver_ticket},
    windows::Window,
};
use ibis_native::paste::{SYSTEM, intended_paste, policy_decision};
use tauri::{AppHandle, Manager};

/// The grant checks the permission guide makes.
#[cfg(target_os = "macos")]
mod macos {
    pub(super) use ibis_native::paste::macos::{accessibility_granted, request_accessibility};
}
#[cfg(test)]
mod notepad_test;
pub(crate) mod permission;
#[cfg(target_os = "windows")]
mod windows;

/// The app the picker was last opened from, and the owner every clipboard
/// write of the app goes through for its token.
pub(crate) struct LastTarget(ibis_native::paste::LastTarget);

impl Default for LastTarget {
    fn default() -> Self {
        Self(ibis_native::paste::LastTarget::new(crate::copy::write_token))
    }
}

impl Deref for LastTarget {
    type Target = ibis_native::paste::LastTarget;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// The app around a macOS paste.
#[cfg(target_os = "macos")]
#[derive(Clone)]
struct Host(AppHandle);

#[cfg(target_os = "macos")]
impl ibis_native::paste::macos::Host for Host {
    fn run_on_main(&self, work: impl FnOnce() + Send + 'static) -> bool {
        self.0.run_on_main_thread(work).is_ok()
    }

    fn last_target(&self) -> &ibis_native::paste::LastTarget {
        &self.0.state::<LastTarget>().inner().0
    }

    fn picker_hidden(&self) -> bool {
        self.0
            .get_webview_window("main")
            .is_some_and(|window| window.is_visible().ok() == Some(false))
    }

    fn open_paste_guide(&self) {
        crate::settings_window::open(&self.0, Some(crate::settings_window::Page::Paste));
    }

    fn paste_key(&self) -> Option<u16> {
        crate::picker::command_key(tauri_plugin_global_shortcut::Code::KeyV, crate::picker::typing())
    }

    fn paste_choice(&self, id: &AppId, identity: Option<&str>) -> Option<(Option<AppPaste>, bool)> {
        let preference = self
            .0
            .state::<crate::destination::AppMemory>()
            .paste_for(id, identity)
            .ok()?;
        Some((preference, paste_setting(&self.0)?))
    }
}

/// The paste setting as the settings file holds it now, or `None` when the
/// file cannot be used for consent: delivery then only copies. A damaged
/// file never falls back to its backup's choice.
fn paste_setting(app: &AppHandle) -> Option<bool> {
    read_paste_setting(&app.state::<crate::preferences::SettingsFile>().path)
}

fn read_paste_setting(path: &std::path::Path) -> Option<bool> {
    paste_consent(&ibis_engine::settings::load(path))
}

fn paste_consent(loaded: &ibis_engine::settings::Loaded) -> Option<bool> {
    Some(loaded.cites().ok()?.portable.paste())
}

/// Turning Paste off, by any route (Settings, the picker, a restore or an
/// edit to the file), revokes every copy already on its way to a paste: a
/// detached ticket's final checks then refuse. So does a settings file that
/// can no longer give consent. Delivery also reads the setting when it starts.
pub(crate) fn revoke_when_paste_changes(app: &AppHandle) {
    let file = app.state::<crate::preferences::SettingsFile>();
    let known = std::sync::Mutex::new(paste_consent(&file.owner.view()));
    let app = app.clone();
    file.owner.on_publish(move |loaded| {
        if consent_changed(&known, paste_consent(loaded)) {
            let _ = change_destination(&app, true, |_| Ok(()));
        }
    });
}

/// Whether consent differs from the last view; a poisoned record counts as changed.
fn consent_changed(known: &std::sync::Mutex<Option<bool>>, now: Option<bool>) -> bool {
    known
        .lock()
        .map_or(true, |mut known| std::mem::replace(&mut *known, now) != now)
}

/// Serialize successful policy changes with proof publication and native
/// posting. Detached workers keep a revocable epoch after taking their proof.
pub(crate) fn change_destination<T, R: tauri::Runtime>(
    app: &AppHandle<R>,
    revoke: bool,
    change: impl FnOnce(Option<&Captured>) -> Result<T, String>,
) -> Result<T, String> {
    app.state::<LastTarget>().change_destination(revoke, change)
}

/// Remember the app in front, before the picker shows. Ibis in front keeps the
/// earlier capture. Returns short selected text for the search, once (#1526).
pub(crate) fn capture(app: &AppHandle) -> Option<String> {
    let own = i32::try_from(std::process::id()).unwrap_or(i32::MAX);
    #[cfg(target_os = "macos")]
    let mut front = ibis_native::paste::macos::frontmost(&Host(app.clone()));
    #[cfg(target_os = "macos")]
    let search = front
        .as_mut()
        .and_then(|front| front.search.take())
        .map(capture::SelectedText::into_search);
    #[cfg(not(target_os = "macos"))]
    let search = None;
    #[cfg(target_os = "windows")]
    let generation = app.state::<crate::picker::PickerGeneration>().current();
    #[cfg(target_os = "windows")]
    let (front, window) = windows::capture();
    #[cfg(target_os = "linux")]
    let front = None;
    #[cfg(not(target_os = "windows"))]
    app.state::<LastTarget>()
        .begin_presentation(capture::captured(SYSTEM, front, own));
    #[cfg(target_os = "windows")]
    app.state::<LastTarget>()
        .publish_capture(capture::captured(SYSTEM, front, own), window, generation, || {
            app.state::<crate::picker::PickerGeneration>().current()
        });
    search
}

/// The app the picker was last opened from, with the name the picker shows
/// for it, resolved by the same owner as Settings.
pub(crate) fn last_app(app: &AppHandle) -> Option<(AppId, String, Option<String>)> {
    app.state::<LastTarget>()
        .with_captured(|captured| {
            let captured = captured?;
            let id = captured.target.app.clone()?;
            let name = crate::destination::display_name(&id);
            Some((id, name, captured.app_identity.clone()))
        })
        .flatten()
}

/// The footer's intended action, from the same policy as delivery. This only
/// reads the captured target and permission; secure input is checked at delivery.
pub(crate) fn intends_paste(app: &AppHandle, enabled: bool) -> bool {
    paste_policy(app, enabled, false)
}

/// Whether an ordinary target could paste after explicit approval. It uses the
/// same proof and refusal policy, so unsafe/unproven apps never ask for approval.
pub(crate) fn may_choose_paste(app: &AppHandle, enabled: bool) -> bool {
    paste_policy(app, enabled, true)
}

fn paste_policy(app: &AppHandle, enabled: bool, choosing: bool) -> bool {
    app.state::<LastTarget>()
        .with_captured(|captured| {
            if choosing
                && captured.is_some_and(|target| {
                    target.app_identity.is_none() && target.target.app.as_ref().is_some_and(|id| lookup(id).is_none())
                })
            {
                return false;
            }
            let preference = if choosing {
                Some(AppPaste::Paste)
            } else {
                let Ok(preference) = captured.map_or(Ok(None), |target| {
                    app.state::<crate::destination::AppMemory>().for_capture(target)
                }) else {
                    return false;
                };
                preference
            };
            let permission = PastePermission {
                enabled,
                granted: enabled && permission::paste_permission().granted,
                app: preference,
            };
            // What is selected now says nothing about choosing paste for this app.
            if choosing {
                return captured.is_some_and(|captured| policy_decision(captured, permission) == Decision::Paste);
            }
            intended_paste(captured, permission)
        })
        .unwrap_or(false)
}

/// Freeze a citation's destination at request entry, before clipboard I/O.
pub(crate) fn begin_copy(app: &AppHandle, replaces: &[String]) -> CopyAttempt {
    app.state::<LastTarget>().begin_copy(
        || app.state::<crate::picker::PickerGeneration>().current(),
        replaces,
        |captured| app.state::<crate::destination::AppMemory>().for_capture(captured),
    )
}

#[cfg(target_os = "windows")]
pub(crate) fn take_copy(app: &AppHandle, token: Option<u32>) -> Option<DeliveryTicket<Window>> {
    app.state::<LastTarget>().take_ticket(token)
}

#[cfg(target_os = "windows")]
pub(crate) fn deliver_copy(app: &AppHandle, _text: &str, ticket: Option<DeliveryTicket<Window>>) -> Delivered {
    let desk = windows::Win::new(app);
    // Missing/stale receipts must not strand keys from an earlier copy.
    if !desk.release_keys() {
        return key_release_failed();
    }
    let Some(ticket) = ticket else {
        return Delivered::copied(CopyReason::PasteUnavailable);
    };
    deliver_ticket(&desk, &ticket, || paste_setting(app))
}

/// Paste into the captured app when it is safe to; the notice then says what happened.
#[cfg(not(target_os = "windows"))]
pub(crate) fn deliver_copy(app: &AppHandle, _text: &str, token: Option<u32>) -> Delivered {
    let setting = || paste_setting(app);
    #[cfg(target_os = "macos")]
    return ibis_native::paste::deliver_copy(&Host(app.clone()), setting, token);
    #[cfg(target_os = "linux")]
    ibis_native::paste::deliver_copy(&app.state::<LastTarget>(), setting, token)
}

#[cfg(test)]
mod setting_tests {
    use super::{consent_changed, read_paste_setting};

    #[test]
    fn every_change_of_consent_revokes_and_repeats_do_not() {
        let known = std::sync::Mutex::new(Some(true));
        assert!(!consent_changed(&known, Some(true)), "an unrelated save");
        assert!(consent_changed(&known, Some(false)), "Paste turned off");
        assert!(!consent_changed(&known, Some(false)));
        assert!(consent_changed(&known, None), "a file that cannot give consent");
        assert!(consent_changed(&known, Some(true)), "Paste back on");
    }

    /// Moved from `ibis-native` with the settings read: consent comes only
    /// from a usable settings file, never a backup, across reopenings.
    #[test]
    fn settings_failures_keep_the_citation_copied_after_reopening() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("settings.json");
        assert_eq!(
            read_paste_setting(&path),
            Some(true),
            "first-run defaults still allow paste"
        );
        std::fs::write(&path, r#"{"paste":false}"#).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(read_paste_setting(&path), Some(false));
        for _ in 0..2 {
            std::fs::write(&path, "{").unwrap_or_else(|e| panic!("{e}"));
            std::fs::write(path.with_extension("json.bak"), r#"{"paste":true}"#).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(read_paste_setting(&path), None);
            std::fs::write(&path, format!("{{}}{}", " ".repeat(1024 * 1024))).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(
                read_paste_setting(&path),
                None,
                "oversized settings cannot use backup consent"
            );
            std::fs::remove_file(&path).unwrap_or_else(|e| panic!("{e}"));
            std::fs::create_dir(&path).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(read_paste_setting(&path), None, "failed I/O cannot use backup consent");
            std::fs::remove_dir(&path).unwrap_or_else(|e| panic!("{e}"));
        }
    }
}
