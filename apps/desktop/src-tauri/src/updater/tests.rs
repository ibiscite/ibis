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
fn update_offer_uses_standard_windows_consent_buttons() {
    if cfg!(target_os = "windows") {
        assert!(matches!(offer_buttons(), MessageDialogButtons::YesNo));
    } else {
        match offer_buttons() {
            MessageDialogButtons::OkCancelCustom(yes, no) => {
                assert_eq!((yes.as_str(), no.as_str()), ("Install and Restart", "Later"));
            }
            _ => panic!("the update must offer an explicit accept and decline action"),
        }
    }
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
