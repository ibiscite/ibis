//! Settings owns automatic update checks. One worker saves choices in order,
//! off the main thread, and publishes each saved answer.

use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};

use tauri::{AppHandle, Emitter, Manager};

use super::{CheckRequest, Updates, available, check, tell};
use crate::diagnostics::{NativeIssue, report};
use crate::library::CommandError;
use crate::preferences::SettingsFile;

/// Whether checks run on their own, as last saved.
pub(crate) fn automatic(app: &AppHandle) -> bool {
    app.state::<Updates>().automatic.load(Ordering::SeqCst)
}

/// Apply a saved choice.
pub(super) fn set_automatic(app: &AppHandle, on: bool) {
    app.state::<Updates>().automatic.store(on, Ordering::SeqCst);
}

/// The worker's queue, started on first use and again after a failure.
#[derive(Default)]
pub(super) struct Saver(Mutex<Option<Sender<Choice>>>);

struct Choice {
    on: bool,
    reply: Option<tauri::async_runtime::Sender<Result<(), CommandError>>>,
}

/// A choice from the native offer has no Settings page to handle its failure.
pub(super) fn change(app: &AppHandle, on: bool) {
    if enqueue(app, Choice { on, reply: None }).is_err() {
        tell(app, "Ibis could not change whether it checks for updates. Try again.");
    }
}

fn enqueue(app: &AppHandle, choice: Choice) -> Result<(), CommandError> {
    let updates = app.state::<Updates>();
    let mut queue = updates.saver.0.lock().map_err(|_| CommandError::SettingsUnavailable)?;
    if queue.is_none() {
        *queue = start(app).ok();
    }
    let sent = queue
        .as_ref()
        .map_or(Err(()), |changes| changes.send(choice).map_err(drop));
    if sent.is_err() {
        *queue = None;
    }
    sent.map_err(|()| CommandError::SettingsUnavailable)
}

fn start(app: &AppHandle) -> Result<Sender<Choice>, ()> {
    let (changes, queue) = mpsc::channel();
    let handle = app.clone();
    let worker = std::thread::Builder::new()
        .name("update-setting".to_owned())
        .spawn(move || {
            serve(
                queue,
                |on| save(&handle, on),
                |problem| {
                    tell(
                        &handle,
                        &format!("Your choice could not be saved, so nothing changed: {problem}."),
                    );
                },
            )
        });
    report(worker.as_ref().map(drop), NativeIssue::UpdateSetting);
    worker.map(|_| changes).map_err(drop)
}

fn serve(
    queue: Receiver<Choice>,
    mut save: impl FnMut(bool) -> Result<(), CommandError>,
    notify: impl Fn(CommandError),
) {
    for Choice { on, reply } in queue {
        let saved = save(on);
        if let Some(reply) = reply {
            let _ = reply.try_send(saved);
        } else if let Err(problem) = saved {
            notify(problem);
        }
    }
}

/// Saved first, then applied: Settings and the running app change only once
/// the choice is on disk, so quitting at any moment never loses a choice that
/// was shown as made. Settings hears what applies either way.
fn save(app: &AppHandle, on: bool) -> Result<(), CommandError> {
    let file = app.state::<SettingsFile>();
    file.owner.change_with(
        |settings| {
            let before = settings.clone();
            settings.device.update_checks = Some(on);
            Ok::<_, ibis_engine::settings::SettingsError>(before)
        },
        |result| {
            result
                .map(|(saved, before)| {
                    file.remember_native_changes(&before, &saved);
                    set_automatic(app, on);
                })
                .map_err(crate::library::CommandError::from)
        },
    )?;
    crate::library::refresh_settings_notice(app, &app.state::<SettingsFile>());
    publish(app);
    Ok(())
}

fn publish(app: &AppHandle) {
    for window in ["main", crate::settings_window::LABEL] {
        report(
            app.emit_to(window, "update:automatic", automatic(app)),
            NativeIssue::UpdateSetting,
        );
    }
}

/// Apply an explicit live-file choice without another save or consent prompt.
pub(crate) fn apply_file_setting(app: &AppHandle, saved: Option<bool>) {
    app.state::<Updates>().question.store(false, Ordering::SeqCst);
    set_automatic(app, saved.unwrap_or(false));
    publish(app);
}

/// Whether update checks run on their own; `None` when this build has no updates.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn update_checks(app: AppHandle) -> Option<bool> {
    available(&app).then(|| automatic(&app))
}

/// Turn automatic checks on or off from Settings and answer this specific write.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn set_update_checks(app: AppHandle, enabled: bool) -> Result<(), CommandError> {
    if !available(&app) {
        return Err(CommandError::SettingsUnavailable);
    }
    let (send, mut receive) = tauri::async_runtime::channel(1);
    enqueue(
        &app,
        Choice {
            on: enabled,
            reply: Some(send),
        },
    )?;
    receive.recv().await.ok_or(CommandError::SettingsUnavailable)?
}

/// The tray's and menu bar's "Check for Updates…": check now and answer in one
/// native prompt, never by opening Settings. An update found is the offer
/// (install now or later); otherwise the prompt says what the check found.
pub(crate) fn check_from_menu(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let said = if available(&app) {
            menu_answer(check(&app, CheckRequest::Menu).await)
        } else {
            Some("about-unavailable")
        };
        if let Some(id) = said {
            // A menu click leaves Ibis inactive; the prompt must come to the front.
            #[cfg(target_os = "macos")]
            crate::panel::activate_for_dialog(&app);
            tell(
                &app,
                &ibis_engine::catalogue::text(id, None, &[]).unwrap_or_else(|| id.to_owned()),
            );
        }
    });
}

/// What the menu's check says, by catalogue id; an offer speaks for itself.
fn menu_answer(result: super::CheckResult) -> Option<&'static str> {
    match result {
        super::CheckResult::Offered => None,
        super::CheckResult::Newest => Some("about-check-newest"),
        super::CheckResult::NotRun => Some("about-check-busy"),
        super::CheckResult::Failed => Some("about-check-failed"),
    }
}

/// Settings' Check for Updates, in About: the result shows beside the button.
/// An update found still opens the offer.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn check_for_updates(app: AppHandle, opening: u64) -> super::CheckResult {
    if available(&app) {
        check(&app, CheckRequest::Settings(opening)).await
    } else {
        super::CheckResult::NotRun
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_check_answers_every_outcome_in_its_prompt_and_an_offer_alone() {
        use super::super::CheckResult;
        assert_eq!(menu_answer(CheckResult::Offered), None, "the offer is the prompt");
        for (result, id) in [
            (CheckResult::Newest, "about-check-newest"),
            (CheckResult::NotRun, "about-check-busy"),
            (CheckResult::Failed, "about-check-failed"),
        ] {
            assert_eq!(menu_answer(result), Some(id));
            let said = ibis_engine::catalogue::text(id, None, &[]);
            assert!(said.is_some_and(|text| !text.is_empty()), "{id} is in the catalogue");
        }
    }

    #[test]
    fn changes_are_saved_in_the_order_they_were_made() -> Result<(), Box<dyn std::error::Error>> {
        let (changes, queue) = mpsc::channel();
        let mut replies = Vec::new();
        for on in [true, false, false, true] {
            let (send, receive) = tauri::async_runtime::channel(1);
            changes
                .send(Choice { on, reply: Some(send) })
                .map_err(|_| "queue closed")?;
            replies.push(receive);
        }
        drop(changes);
        let mut saved = Vec::new();
        serve(
            queue,
            |on| {
                saved.push(on);
                if on {
                    Ok(())
                } else {
                    Err(CommandError::SettingsUnavailable)
                }
            },
            |_| panic!("a Settings refusal belongs to its caller"),
        );
        assert_eq!(saved, [true, false, false, true]);
        assert_eq!(
            replies
                .iter_mut()
                .map(|reply| reply.try_recv().unwrap().is_ok())
                .collect::<Vec<_>>(),
            saved
        );
        Ok(())
    }
}
