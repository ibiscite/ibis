//! Retain update offers until the person is using the picker.

use super::*;
use crate::picker::{ChooserLease, ChooserOpen};

pub(super) struct Pending {
    version: String,
    text: String,
    install: Box<dyn FnOnce(BusyGuard) + Send>,
    busy: BusyGuard,
}

/// Tests replace only native visibility/focus queries and dialog presentation.
pub(super) trait OfferUi: tauri::Runtime {
    fn visible(parent: &tauri::WebviewWindow<Self>) -> bool;
    fn focused(parent: &tauri::WebviewWindow<Self>) -> bool;
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

pub(super) fn queue<R: OfferUi + InstallationUi>(app: &AppHandle<R>, update: Update, busy: BusyGuard) {
    let handle = app.clone();
    retain(
        app,
        Pending {
            version: update.version.clone(),
            text: offer_text(&app.package_info().version.to_string(), &update),
            install: Box::new(move |busy| {
                tauri::async_runtime::spawn(async move { install_update(&handle, update, busy).await });
            }),
            busy,
        },
    );
}

fn retain<R: OfferUi>(app: &AppHandle<R>, offer: Pending) {
    if let Ok(mut pending) = app.state::<Updates>().offer.lock() {
        *pending = Some(offer);
    }
    present_pending(app);
}

pub(super) fn present_pending<R: OfferUi>(app: &AppHandle<R>) {
    let handle = app.clone();
    report(
        app.run_on_main_thread(move || {
            // Recheck on the main thread: the person may have dismissed the
            // picker while the feed request or this task was in flight.
            let Some(parent) = handle.get_webview_window("main") else {
                return;
            };
            if !R::visible(&parent) || !R::focused(&parent) {
                return;
            }
            let Some(owner) = handle.state::<ChooserOpen>().begin() else {
                return;
            };
            let pending = handle.state::<Updates>().offer.lock().ok().and_then(|mut p| p.take());
            let Some(Pending {
                version,
                text,
                install,
                busy,
            }) = pending
            else {
                return;
            };
            let callback = handle.clone();
            R::present(&handle, &parent, owner, text, move |accepted| {
                if accepted {
                    // The retained action starts only after explicit consent.
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
mod tests {
    use super::*;

    type Answer = Box<dyn FnOnce(bool) + Send>;

    /// Tauri's mock always reports visible and unfocused, even after hide().
    /// Model those OS queries here; production offer retention and dispatch run
    /// unchanged, without opening a native window or downloading a package.
    #[derive(Default)]
    struct NativePicker {
        visible: AtomicBool,
        focused: AtomicBool,
        answers: Mutex<Vec<Answer>>,
    }

    impl OfferUi for tauri::test::MockRuntime {
        fn visible(parent: &tauri::WebviewWindow<Self>) -> bool {
            parent.state::<NativePicker>().visible.load(Ordering::SeqCst)
        }

        fn focused(parent: &tauri::WebviewWindow<Self>) -> bool {
            parent.state::<NativePicker>().focused.load(Ordering::SeqCst)
        }

        fn present(
            app: &AppHandle<Self>,
            parent: &tauri::WebviewWindow<Self>,
            owner: ChooserLease,
            text: String,
            answer: impl FnOnce(bool) + Send + 'static,
        ) {
            assert_eq!(parent.label(), "main");
            assert!(Self::visible(parent));
            assert!(Self::focused(parent));
            assert!(text.contains("9.0.0"));
            app.state::<NativePicker>()
                .answers
                .lock()
                .unwrap_or_else(|e| panic!("{e}"))
                .push(Box::new(move |yes| {
                    drop(owner);
                    answer(yes);
                }));
        }
    }

    #[test]
    fn update_available_while_picker_is_hidden_waits_without_showing_or_focusing_it()
    -> Result<(), Box<dyn std::error::Error>> {
        let app = tauri::test::mock_builder()
            .manage(Updates::default())
            .manage(ChooserOpen::default())
            .manage(NativePicker::default())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))?;
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .visible(false)
            .build()?;
        let picker = app.state::<NativePicker>();
        let updates = app.state::<Updates>();
        let dialogs = app.state::<ChooserOpen>();
        let busy = updates.begin().unwrap_or_else(|| panic!("check"));
        retain(
            app.handle(),
            Pending {
                version: "9.0.0".into(),
                text: "Ibis 9.0.0 is available".into(),
                install: Box::new(|_| panic!("an offer must not install without consent")),
                busy,
            },
        );
        present_pending(app.handle());
        assert!(!picker.visible.load(Ordering::SeqCst));
        assert!(!picker.focused.load(Ordering::SeqCst));
        assert!(picker.answers.lock().unwrap_or_else(|e| panic!("{e}")).is_empty());
        assert!(updates.offer.lock().unwrap_or_else(|e| panic!("{e}")).is_some());
        assert!(updates.begin().is_none(), "a retained offer still owns the check");
        assert!(
            !dialogs.is_open(),
            "hidden offers do not acquire native dialog ownership"
        );

        // A stale focus report must not make a hidden picker eligible either.
        picker.focused.store(true, Ordering::SeqCst);
        present_pending(app.handle());
        assert!(!picker.visible.load(Ordering::SeqCst));
        assert!(picker.answers.lock().unwrap_or_else(|e| panic!("{e}")).is_empty());
        picker.focused.store(false, Ordering::SeqCst);

        // Focus may arrive after show() on Windows/Linux. A pinned picker can
        // also stay visible while the person uses another app.
        picker.visible.store(true, Ordering::SeqCst);
        present_pending(app.handle());
        assert!(!picker.focused.load(Ordering::SeqCst));
        assert!(picker.answers.lock().unwrap_or_else(|e| panic!("{e}")).is_empty());

        // The later focus event retries, but a file chooser already owns it.
        picker.focused.store(true, Ordering::SeqCst);
        let chooser = dialogs.begin().unwrap_or_else(|| panic!("file chooser"));
        present_pending(app.handle());
        assert!(updates.offer.lock().unwrap_or_else(|e| panic!("{e}")).is_some());
        drop(chooser);
        present_pending(app.handle());
        present_pending(app.handle()); // no duplicate sheet on another show
        assert!(updates.offer.lock().unwrap_or_else(|e| panic!("{e}")).is_none());
        assert!(dialogs.is_open());
        let mut answers = picker.answers.lock().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(answers.len(), 1);
        let later = answers.pop().unwrap_or_else(|| panic!("offer"));
        drop(answers);
        later(false);
        assert!(!dialogs.is_open());
        assert!(updates.begin().is_some(), "Later releases the check for retry");
        assert_eq!(
            updates.declined.lock().unwrap_or_else(|e| panic!("{e}")).as_deref(),
            Some("9.0.0")
        );
        drop(window);
        Ok(())
    }
}
