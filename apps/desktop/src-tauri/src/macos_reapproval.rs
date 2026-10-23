//! Deliver ADR0023's informational notice only beside a visible picker.

use std::path::Path;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogResult};

use crate::diagnostics::{NativeIssue, checked, report};
use crate::identity::{Reapproval, acknowledge_macos_login_reapproval, macos_login_reapproval};
use crate::picker::ChooserOpen;

const MESSAGE: &str = "Ibis has a new app identity. Your library and shortcut settings are preserved. To open Ibis when you log in, turn on Open at Login in More.";

/// Separate from dialog ownership: covers receipt work and queued presentation.
#[derive(Default)]
pub(crate) struct Delivery(ChooserOpen);

/// Called only after the native picker has actually been shown, never at login.
pub(crate) fn presented(app: &AppHandle) {
    let Some(attempt) = app.state::<Delivery>().0.begin() else {
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Ok(directory) = app.path().app_config_dir() else {
            report(Err::<(), ()>(()), NativeIssue::ReapprovalNotice);
            return;
        };
        let identifier = app.config().identifier.clone();
        match macos_login_reapproval(&directory, &identifier) {
            Ok(Reapproval::Pending) => {}
            Ok(_) => return,
            Err(_) => {
                report(Err::<(), ()>(()), NativeIssue::ReapprovalNotice);
                return;
            }
        }
        let handle = app.clone();
        report(
            app.run_on_main_thread(move || {
                // Take ownership before rechecking visibility, because the
                // focus changes caused by an attached sheet must not hide it.
                let Some(owner) = handle.state::<ChooserOpen>().begin() else {
                    return;
                };
                let Some(window) = checked(handle.get_webview_window("main").ok_or(()), NativeIssue::Window) else {
                    return;
                };
                if checked(window.is_visible(), NativeIssue::Visibility) != Some(true) {
                    return;
                }
                // ns_view validates the same AppKit window handle that the
                // dialog parent uses. Tao's AppKit display handle is infallible.
                if checked(window.ns_view(), NativeIssue::Window).is_none() {
                    return;
                }
                crate::panel::activate_for_dialog(&handle);
                handle
                    .dialog()
                    .message(MESSAGE)
                    .title("Open at Login")
                    .buttons(MessageDialogButtons::Ok)
                    .parent(&window)
                    .show_with_result(move |result| {
                        let _owner = owner;
                        tauri::async_runtime::spawn_blocking(move || {
                            let _attempt = attempt;
                            report(
                                acknowledge(result, &directory, &identifier),
                                NativeIssue::ReapprovalNotice,
                            );
                        });
                    });
            }),
            NativeIssue::ReapprovalNotice,
        );
    });
}

fn acknowledge(result: MessageDialogResult, directory: &Path, identifier: &str) -> std::io::Result<()> {
    if matches!(result, MessageDialogResult::Ok) {
        acknowledge_macos_login_reapproval(directory, identifier)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{Reapproval, macos_login_reapproval};

    fn pending(directory: &Path) -> std::io::Result<()> {
        std::fs::create_dir(directory)?;
        std::fs::write(
            directory.join("macos-login-reapproval-v1.pending"),
            b"macos-open-at-login-reapproval-v1\n",
        )
    }

    #[test]
    fn only_positive_dismissal_acknowledges_the_exact_channel() -> std::io::Result<()> {
        for identifier in ["com.ibiscite.ibis", "com.ibiscite.ibis.nightly"] {
            let root = tempfile::tempdir()?;
            let directory = root.path().join(identifier);
            pending(&directory)?;
            for result in [
                MessageDialogResult::Cancel,
                MessageDialogResult::Yes,
                MessageDialogResult::No,
                MessageDialogResult::Custom("OK".into()),
            ] {
                acknowledge(result, &directory, identifier)?;
                assert_eq!(macos_login_reapproval(&directory, identifier)?, Reapproval::Pending);
            }
            acknowledge(MessageDialogResult::Ok, &directory, identifier)?;
            assert_eq!(
                macos_login_reapproval(&directory, identifier)?,
                Reapproval::Acknowledged
            );
        }
        Ok(())
    }

    #[test]
    fn dropped_delivery_preserves_pending_and_releases_both_leases() -> std::io::Result<()> {
        let root = tempfile::tempdir()?;
        let identifier = "com.ibiscite.ibis";
        let directory = root.path().join(identifier);
        pending(&directory)?;
        let delivery = Delivery::default();
        let dialogs = ChooserOpen::default();
        let attempt = delivery.0.begin().unwrap_or_else(|| panic!("first attempt"));
        let owner = dialogs.begin().unwrap_or_else(|| panic!("first owner"));
        assert!(delivery.0.begin().is_none());
        let callback = || {
            let (_attempt, _owner) = (attempt, owner);
            acknowledge(MessageDialogResult::Ok, &directory, identifier)
        };
        drop(callback);
        assert_eq!(macos_login_reapproval(&directory, identifier)?, Reapproval::Pending);
        assert!(delivery.0.begin().is_some());
        assert!(dialogs.begin().is_some());
        Ok(())
    }

    #[test]
    fn failed_acknowledgement_preserves_the_obligation() -> std::io::Result<()> {
        let root = tempfile::tempdir()?;
        let identifier = "com.ibiscite.ibis";
        let directory = root.path().join(identifier);
        pending(&directory)?;
        let acknowledged = directory.join("macos-login-reapproval-v1.acknowledged");
        std::fs::create_dir(&acknowledged)?;
        assert!(acknowledge(MessageDialogResult::Ok, &directory, identifier).is_err());
        assert!(directory.join("macos-login-reapproval-v1.pending").is_file());
        assert!(acknowledged.is_dir());
        Ok(())
    }
}
