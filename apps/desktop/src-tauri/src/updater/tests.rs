use super::*;
use serde_json::json;

#[test]
fn busy_guard_releases_dropped_callbacks_unwinding_and_cancelled_futures() {
    let updates = Updates::default();
    let busy = updates.begin().unwrap_or_else(|| panic!("first check"));
    assert!(updates.begin().is_none(), "concurrent checks must be refused");
    let callback = move || drop(busy);
    drop(callback);
    assert!(!updates.busy.load(Ordering::SeqCst));

    let unwound = std::panic::catch_unwind(|| {
        let _busy = updates.begin().unwrap_or_else(|| panic!("later check"));
        panic!("synthetic operation failure");
    });
    assert!(unwound.is_err());
    assert!(!updates.busy.load(Ordering::SeqCst));

    let busy = updates.begin().unwrap_or_else(|| panic!("later install"));
    let mut operation = Box::pin(async move {
        let _busy = busy;
        std::future::pending::<()>().await;
    });
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(std::future::Future::poll(operation.as_mut(), &mut context).is_pending());
    assert!(updates.begin().is_none());
    drop(operation);
    assert!(updates.begin().is_some(), "cancellation must permit retry");
}

#[test]
fn installation_recovery_waits_for_an_exclusive_owner() {
    let updates = Updates::default();
    let failure = &updates.installation_failure;
    let dialogs = crate::picker::ChooserOpen::default();
    assert!(failure.begin(&dialogs).is_none());
    failure.0.store(true, Ordering::SeqCst);
    let chooser = dialogs.begin().unwrap_or_else(|| panic!("chooser owner"));
    assert!(failure.begin(&dialogs).is_none());
    drop(chooser);
    let owner = failure.begin(&dialogs).unwrap_or_else(|| panic!("later actual show"));
    // An actual-show callback during initial presentation cannot recurse.
    assert!(failure.begin(&dialogs).is_none());
    drop(owner); // missing/hidden/unavailable parent leaves recovery pending
    assert!(failure.begin(&dialogs).is_some());
}

#[test]
fn only_ok_dismisses_installation_recovery_and_drop_releases_blur() {
    let updates = Updates::default();
    let failure = &updates.installation_failure;
    let dialogs = crate::picker::ChooserOpen::default();
    failure.0.store(true, Ordering::SeqCst);
    for result in [
        MessageDialogResult::Cancel,
        MessageDialogResult::Yes,
        MessageDialogResult::No,
        MessageDialogResult::Custom("OK".into()),
    ] {
        let owner = failure.begin(&dialogs).unwrap_or_else(|| panic!("pending owner"));
        let callback = || {
            let _owner = owner;
            failure.answered(result);
        };
        callback();
        assert!(!dialogs.is_open());
        assert!(failure.0.load(Ordering::SeqCst));
    }
    let owner = failure.begin(&dialogs).unwrap_or_else(|| panic!("pending owner"));
    drop(move || {
        let _owner = owner;
        failure.answered(MessageDialogResult::Ok);
    });
    assert!(!dialogs.is_open());
    // A dropped native callback must not acknowledge an unseen message.
    let owner = failure.begin(&dialogs).unwrap_or_else(|| panic!("still pending"));
    owner.during(|| failure.answered(MessageDialogResult::Ok));
    assert!(!dialogs.is_open());
    assert!(failure.begin(&dialogs).is_none());
}

#[test]
fn installation_failure_offers_recovery_without_claiming_rollback() {
    assert!(!INSTALL_FAILURE_MESSAGE.contains("unchanged"));
    assert!(INSTALL_FAILURE_MESSAGE.contains("Try again"));
    assert!(INSTALL_FAILURE_MESSAGE.contains("download"));
}

#[test]
fn update_offer_requires_the_picker_instead_of_another_owned_window() -> Result<(), Box<dyn std::error::Error>> {
    let app = tauri::test::mock_builder().build(tauri::test::mock_context(tauri::test::noop_assets()))?;
    assert!(offer_parent(app.handle()).is_none());
    let _notice = tauri::WebviewWindowBuilder::new(&app, "notice", Default::default()).build()?;
    assert!(offer_parent(app.handle()).is_none());
    let _picker = tauri::WebviewWindowBuilder::new(&app, "main", Default::default()).build()?;
    assert_eq!(
        offer_parent(app.handle()).map(|window| window.label().to_owned()),
        Some("main".into())
    );
    Ok(())
}

#[test]
fn update_offer_defaults_to_later_and_never_maps_cancellation_to_install() {
    match offer_buttons() {
        MessageDialogButtons::YesNoCancelCustom(first, install, cancel) => {
            assert_eq!(
                (first.as_str(), install.as_str(), cancel.as_str()),
                ("Later", "Install and Restart", "Cancel")
            );
        }
        _ => panic!("Return and cancellation must have safe answers; Install is a separate action"),
    }
}

#[test]
fn update_offer_return_escape_and_close_are_safe_and_install_is_deliberate() {
    let MessageDialogButtons::YesNoCancelCustom(later, install, cancel) = offer_buttons() else {
        panic!("separate default, install and cancellation actions");
    };
    // Locked rfd's Windows custom TaskDialog and macOS NSAlert select the
    // first action for Return. The plugin maps dismissal to the cancel slot.
    for result in [
        MessageDialogResult::Custom(later),
        MessageDialogResult::Custom(cancel),
        MessageDialogResult::Cancel,
        MessageDialogResult::Ok,
        MessageDialogResult::Yes,
        MessageDialogResult::No,
    ] {
        assert!(!offer_accepted(result));
    }
    assert!(offer_accepted(MessageDialogResult::Custom(install)));
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires an isolated GTK display; exercise during native qualification"]
fn gtk_offer_return_declines_and_explicit_install_button_accepts() -> Result<(), Box<dyn std::error::Error>> {
    use gtk::prelude::*;
    gtk::init()?;
    let settle = |stage: &str, ready: &dyn Fn() -> bool| {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !ready() {
            assert!(
                std::time::Instant::now() < deadline,
                "GTK offer did not settle: {stage}"
            );
            gtk::glib::MainContext::default().iteration(false);
            std::thread::yield_now();
        }
    };
    // Production closes the dialog after either answer. Use a fresh dialog
    // for each gesture, rather than moving focus on an already-cancelled one.
    for deliberate_install in [false, true] {
        let (dialog, later) = gtk_offer_dialog("Synthetic update is available".into());
        let install = dialog
            .widget_for_response(gtk::ResponseType::Accept)
            .expect("Install response button");
        let button = install
            .clone()
            .downcast::<gtk::Button>()
            .expect("Install is a native button");
        assert_eq!(button.label().as_deref(), Some("Install and Restart"));
        let answers = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let observed = answers.clone();
        dialog.connect_response(move |_, response| observed.borrow_mut().push(response));
        dialog.show_all();
        settle("mapped dialog", &|| dialog.is_realized() && dialog.is_mapped());
        later.grab_focus();
        settle("Later focus", &|| dialog.focused_widget().as_ref() == Some(&later));
        assert_eq!(dialog.focused_widget().as_ref(), Some(&later));
        assert_eq!(dialog.default_widget().as_ref(), Some(&later));
        if deliberate_install {
            // Real Tab/Space navigation is covered by the managed XTEST GTK
            // regression. Bare Xvfb has no WM; emit("move-focus") is not a
            // substitute for that keyboard path. Activate the native button.
            install.grab_focus();
            settle("Install focus", &|| dialog.focused_widget().as_ref() == Some(&install));
            assert_eq!(dialog.focused_widget().as_ref(), Some(&install));
            assert_eq!(
                dialog.default_widget().as_ref(),
                Some(&later),
                "Install must never become the default"
            );
            button.emit_clicked();
        } else {
            // Exercise GtkWindow's Return/default activation, not the answer
            // callback or a manufactured GtkDialog response.
            assert!(dialog.activate_default());
        }
        settle("one response", &|| answers.borrow().len() == 1);
        assert_eq!(
            answers.borrow().as_slice(),
            &[if deliberate_install {
                gtk::ResponseType::Accept
            } else {
                gtk::ResponseType::Cancel
            }]
        );
        assert_eq!(dialog.default_widget().as_ref(), Some(&later));
        dialog.close();
        settle("closed dialog", &|| !dialog.is_visible());
    }
    Ok(())
}

#[test]
fn automatic_checks_log_a_fixed_outcome_never_the_error_text() {
    use tauri_plugin_updater::Error as E;
    let private = "https://example.invalid/private/path";
    for (error, kind) in [
        (E::Network(private.into()), "network"),
        (E::ReleaseNotFound, "no update file"),
        (E::TargetNotFound(private.into()), "no package for this system"),
        (E::TempDirNotFound, "other"),
    ] {
        let line = automatic_outcome(&Err(failure_kind(&error)));
        assert_eq!(line, format!("automatic update check failed: {kind}"));
        assert!(!line.contains("example"));
    }
    assert_eq!(automatic_outcome(&Ok(None)), "automatic update check: up to date");
}

#[test]
fn updates_are_off_without_an_update_key() {
    assert!(!has_update_key(None));
    assert!(!has_update_key(Some(&json!({}))));
    assert!(!has_update_key(Some(&json!({ "pubkey": " " }))));
    assert!(has_update_key(Some(&json!({ "pubkey": "dW50cnVzdGVk" }))));
}

#[test]
fn the_shipped_config_reads_only_the_fixed_stable_feed() {
    let config: serde_json::Value = serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap_or_default();
    let updater = &config["plugins"]["updater"];
    assert_eq!(
        updater["endpoints"],
        json!(["https://github.com/ibiscite/ibis/releases/download/stable/latest.json"])
    );
    assert!(has_update_key(Some(updater)), "the Ibis update key must be built in");
    assert_eq!(updater["requireSignedVersion"], true);
    assert!(updater.get("dangerousInsecureTransportProtocol").is_none());
}

#[test]
fn the_question_says_what_is_sent() {
    for words in ["GitHub", "IP address", "version", "Nothing from your library"] {
        assert!(QUESTION.contains(words), "missing {words}");
    }
}

#[test]
fn long_release_notes_are_shortened() {
    assert_eq!(shorten("short", 600), "short");
    let long = "é".repeat(700);
    let short = shorten(&long, 600);
    assert_eq!(short.chars().count(), 600);
    assert!(short.ends_with('…'));
}
