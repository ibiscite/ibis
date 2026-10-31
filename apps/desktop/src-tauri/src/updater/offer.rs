//! Quiet availability, fresh checks on return, and immediate explicit offers.

use super::*;
use crate::picker::{ChooserLease, ChooserOpen};

/// Readiness belongs to the last interaction, never to a delayed idle reply.
/// Unknown input state is busy. No query or document text leaves the webview.
#[derive(Default)]
pub(super) struct InputGate {
    serial: u32,
    idle: bool,
}

impl InputGate {
    fn activity(&mut self) -> Option<u32> {
        self.idle = false;
        self.serial = self.serial.checked_add(1)?;
        (self.serial < u32::MAX).then_some(self.serial)
    }

    fn settle(&mut self, ticket: u32) -> bool {
        if self.idle || ticket != self.serial || ticket == 0 || ticket == u32::MAX {
            return false;
        }
        self.idle = true;
        true
    }
}

pub(crate) fn picker_activity(app: &AppHandle) {
    if let Some(updates) = app.try_state::<Updates>()
        && let Ok(mut input) = updates.input.lock()
    {
        let _ = input.activity();
    }
}

/// None records input; only its returned ticket may report a later idle moment.
pub(crate) fn update_picker_activity(window: tauri::WebviewWindow, ticket: Option<u32>) -> Option<u32> {
    if window.label() != "main" {
        return None;
    }
    let updates = window.state::<Updates>();
    let mut input = updates.input.lock().ok()?;
    match ticket {
        None => input.activity(),
        Some(ticket) if input.settle(ticket) => {
            drop(input);
            refresh_pending(window.app_handle());
            Some(ticket)
        }
        Some(_) => None,
    }
}

/// A fresh check's result, consumed by this presentation attempt. Never retained
/// in application state: nightly package URLs can change while Ibis is hidden.
pub(super) struct Candidate {
    version: String,
    text: String,
    install: Box<dyn FnOnce(BusyGuard) + Send>,
}

impl Candidate {
    pub(super) fn new<R: InstallationUi>(app: &AppHandle<R>, update: Update) -> Self {
        let handle = app.clone();
        Self {
            version: update.version.clone(),
            text: offer_text(&app.package_info().version.to_string(), &update),
            install: Box::new(move |busy| {
                tauri::async_runtime::spawn(async move { install_update(&handle, update, busy).await });
            }),
        }
    }
}

pub(crate) fn menu_label(available: bool) -> &'static str {
    if available {
        "Update available…"
    } else {
        "Check for Updates…"
    }
}

/// Tests replace native window/menu actions and the feed request; check
/// ownership, result handling and the visibility/focus gate stay shared.
pub(super) trait OfferUi: tauri::Runtime {
    fn visible(parent: &tauri::WebviewWindow<Self>) -> bool;
    fn focused(parent: &tauri::WebviewWindow<Self>) -> bool;
    fn show_picker(app: &AppHandle<Self>);
    fn refresh(app: &AppHandle<Self>);
    fn availability_changed(app: &AppHandle<Self>, available: bool);
    fn tell(app: &AppHandle<Self>, text: &str);
    fn present(
        app: &AppHandle<Self>,
        parent: &tauri::WebviewWindow<Self>,
        owner: ChooserLease,
        text: String,
        answer: impl FnOnce(bool) + Send + 'static,
    );
}

impl OfferUi for tauri::Wry {
    fn visible(parent: &tauri::WebviewWindow<Self>) -> bool {
        crate::diagnostics::checked(parent.is_visible(), NativeIssue::Visibility) == Some(true)
    }

    fn focused(parent: &tauri::WebviewWindow<Self>) -> bool {
        crate::picker::focused_for_shortcut(parent) == Some(true)
    }

    fn show_picker(app: &AppHandle<Self>) {
        crate::picker::show_picker(app);
    }

    fn refresh(app: &AppHandle<Self>) {
        let app = app.clone();
        tauri::async_runtime::spawn(async move { super::check(&app, false).await });
    }

    fn availability_changed(app: &AppHandle<Self>, available: bool) {
        if let Some(item) = app.try_state::<UpdateMenuItem>() {
            report(item.0.set_text(menu_label(available)), NativeIssue::Window);
        }
    }

    fn tell(app: &AppHandle<Self>, text: &str) {
        tell(app, text);
    }

    fn present(
        app: &AppHandle<Self>,
        parent: &tauri::WebviewWindow<Self>,
        owner: ChooserLease,
        text: String,
        answer: impl FnOnce(bool) + Send + 'static,
    ) {
        present_offer(app, parent, owner, text, answer);
    }
}

fn publish_available<R: OfferUi>(app: &AppHandle<R>, available: bool) {
    if app.state::<Updates>().offer_available.swap(available, Ordering::SeqCst) != available {
        R::availability_changed(app, available);
        report(
            app.emit_to("main", "update:available", available),
            NativeIssue::PickerEvent,
        );
    }
}

pub(super) async fn check<R: OfferUi>(
    app: &AppHandle<R>,
    asked: bool,
    found: impl std::future::Future<Output = Result<Option<Candidate>, &'static str>>,
) {
    let updates = app.state::<Updates>();
    // Re-read after scheduling too: the saved opt-out may have changed since
    // a focus event or periodic worker requested this automatic check.
    if !asked && !updates.automatic.load(Ordering::SeqCst) {
        return;
    }
    let Some(busy) = updates.begin() else {
        if asked {
            R::tell(
                app,
                "Ibis is already checking for updates, or an update is waiting for your answer.",
            );
        }
        return;
    };
    match found.await {
        Ok(Some(candidate)) => {
            let declined = updates.declined.lock().ok().and_then(|d| d.clone());
            if asked || declined.as_deref() != Some(candidate.version.as_str()) {
                present(app, candidate, asked, busy);
            } else {
                publish_available(app, false);
            }
        }
        Ok(None) => {
            publish_available(app, false);
            if asked {
                R::tell(
                    app,
                    &format!("You have the newest version, {}.", app.package_info().version),
                );
            }
        }
        Err(_) if asked => R::tell(
            app,
            "Ibis could not check for updates. Check your internet connection and try again later.",
        ),
        // Keep the availability signal, but never fall back to old metadata.
        Err(_) => {}
    }
}

fn ready<R: OfferUi>(parent: &tauri::WebviewWindow<R>) -> bool {
    R::visible(parent) && R::focused(parent) && parent.state::<Updates>().input.lock().is_ok_and(|input| input.idle)
}

/// Actual show/focus events re-check deferred availability. This never shows a
/// window or uses an old candidate, even when the fresh request fails.
pub(super) fn refresh_pending<R: OfferUi>(app: &AppHandle<R>) {
    let handle = app.clone();
    report(
        app.run_on_main_thread(move || {
            let updates = handle.state::<Updates>();
            if updates.offer_available.load(Ordering::SeqCst)
                && updates.automatic.load(Ordering::SeqCst)
                && !updates.busy.load(Ordering::SeqCst)
                && handle
                    .get_webview_window("main")
                    .is_some_and(|parent| ready::<R>(&parent))
            {
                R::refresh(&handle);
            }
        }),
        NativeIssue::Window,
    );
}

fn present<R: OfferUi>(app: &AppHandle<R>, candidate: Candidate, asked: bool, busy: BusyGuard) {
    let handle = app.clone();
    report(
        app.run_on_main_thread(move || {
            publish_available(&handle, true);
            let Some(parent) = handle.get_webview_window("main") else {
                return;
            };
            // Automatic results never open or focus a window, including when the
            // person dismissed the picker while the fresh check was in flight.
            if !asked && (!handle.state::<Updates>().automatic.load(Ordering::SeqCst) || !ready::<R>(&parent)) {
                return;
            }
            let Some(owner) = handle.state::<ChooserOpen>().begin() else {
                return;
            };
            if asked {
                // Explicit tray/More commands may open their parent. The check and
                // dialog leases prevent show/focus callbacks from starting a retry.
                R::show_picker(&handle);
            }
            publish_available(&handle, false);
            let Candidate { version, text, install } = candidate;
            let callback = handle.clone();
            R::present(&handle, &parent, owner, text, move |accepted| {
                if accepted {
                    install(busy);
                } else if let Ok(mut declined) = callback.state::<Updates>().declined.lock() {
                    *declined = Some(version);
                }
            });
        }),
        NativeIssue::Window,
    );
}

#[cfg(test)]
mod tests;
