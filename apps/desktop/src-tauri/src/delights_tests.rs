use super::*;

#[test]
fn surprises_off_keeps_tips_and_counts_but_does_not_claim_fun_items() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, r#"{"schema_version":2,"small_surprises":false}"#).unwrap();
    let file = SettingsFile::new(path.clone());
    assert_eq!(file.record_delight_paste(1), None);
    let view = file.claim_delights(moment(2026, 11, 1), true, Some(("invented".into(), 100)));
    assert_eq!(view.status, Some(DelightStatus::PasteTip));
    assert!(!view.peck);
    assert!(!view.anniversary);
    for _ in 0..10 {
        assert!(file.claim_about(moment(2026, 11, 1), true).fact.is_none());
    }
    assert!(
        file.delights
            .lock()
            .unwrap()
            .record
            .as_ref()
            .unwrap()
            .libraries
            .is_empty()
    );
    file.flush_delights().unwrap();
    let saved = crate::settings_fixture::read(&path);
    assert_eq!(saved.portable.small_surprises, Some(false));
    assert_eq!(saved.device.delights.unwrap().paste_count, 1);
}

fn moment(year: i32, month: u8, day: u8) -> LocalMoment {
    LocalMoment { year, month, day }
}

#[test]
fn claims_are_once_only_across_windows_failed_writes_and_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let settings = SettingsFile::new(path.clone());
    let now = moment(2026, 11, 1);
    let first = settings.claim_delights(now, true, Some(("invented".into(), 100)));
    assert_eq!(first.status, Some(DelightStatus::Library100));
    assert!(first.peck);
    let second = settings.claim_delights(now, true, Some(("invented".into(), 100)));
    assert_eq!(second.status, None);
    assert!(!second.peck);
    assert_eq!(settings.claim_delights(now, true, None).status, None);
    // B flushes first. Delayed A cannot erase newer claims.
    settings.flush_delights().unwrap();
    let reopened = SettingsFile::new(path);
    let next = reopened.claim_delights(now, true, Some(("invented".into(), 100)));
    assert_eq!(next.status, None);
    assert!(!next.peck);
    // A failed save preserves claims in this process, including another window.
    let blocked = SettingsFile::new(dir.path().join("blocked/settings.json"));
    std::fs::write(dir.path().join("blocked"), "invented obstruction").unwrap();
    let first = blocked.claim_delights(now, true, Some(("invented".into(), 1000)));
    assert!(first.peck);
    assert_eq!(first.status, Some(DelightStatus::Library1000));
    assert!(blocked.flush_delights().is_err());
    let again = blocked.claim_delights(now, true, Some(("invented".into(), 1000)));
    assert!(!again.peck);
    assert_eq!(again.status, None);
    assert_eq!(
        blocked
            .claim_delights(now, true, Some(("invented".into(), 1000)))
            .status,
        None
    );
}

#[test]
fn saved_delights_preserve_other_settings_and_old_files_remain_optional() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let original = serde_json::json!({"schema_version":2,"shortcut":"Alt+Space","invented_future":{"keep":true}});
    std::fs::write(&path, original.to_string()).unwrap();
    let settings = SettingsFile::new(path.clone());
    settings.claim_delights(moment(2026, 11, 1), true, None);
    settings.flush_delights().unwrap();
    settings.update(|s| s.portable.larger_text = Some(true)).unwrap();
    let saved = crate::settings_fixture::read(&path);
    assert_eq!(saved.portable.shortcut.here(), Some("Alt+Space"));
    assert_eq!(saved.unknown["invented_future"], original["invented_future"]);
    assert!(saved.device.delights.is_some());
    assert!(
        serde_json::to_value(ibis_engine::settings::Settings::default())
            .unwrap()
            .get("delights")
            .is_none()
    );
}

#[test]
fn the_build_line_names_the_commit_once() {
    assert_eq!(
        build_line("0.2.7", Some("abc1234"), "2026-11-02"),
        "0.2.7 · abc1234 · 2026-11-02"
    );
    assert_eq!(build_line("0.2.7", None, "2026-11-02"), "0.2.7 · local · 2026-11-02");
    assert_eq!(
        build_line("0.2.8-nightly.20261102120000+abc1234", Some("abc1234def"), "2026-11-02"),
        "0.2.8-nightly.20261102120000+abc1234 · 2026-11-02"
    );
}

#[test]
fn the_shorthand_hint_is_shown_three_times_on_a_computer_then_never() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let file = SettingsFile::new(path.clone());
    assert_eq!(
        (0..5).map(|_| file.claim_shorthand_hint()).collect::<Vec<_>>(),
        [true, true, true, false, false]
    );
    file.flush_delights().unwrap();
    assert_eq!(
        crate::settings_fixture::read(&path)
            .device
            .delights
            .unwrap()
            .shorthand_hints,
        3
    );
}

#[test]
fn the_practice_paste_is_offered_once_with_an_own_library_before_any_paste_even_without_surprises() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, r#"{"schema_version":2,"small_surprises":false}"#).unwrap();
    let file = SettingsFile::new(path.clone());
    let now = moment(2026, 11, 18);
    // No library of the person's own yet (none, or samples): not offered.
    assert!(!file.claim_delights(now, true, None).practice);
    assert!(file.claim_delights(now, true, Some(("invented".into(), 3))).practice);
    assert!(!file.claim_delights(now, true, Some(("invented".into(), 3))).practice);
    file.flush_delights().unwrap();
    assert!(
        crate::settings_fixture::read(&path)
            .device
            .delights
            .unwrap()
            .practice_offered
    );
    // Someone who has already pasted is never offered it.
    let pasted = SettingsFile::new(dir.path().join("other.json"));
    let _ = pasted.record_delight_paste(1);
    assert!(!pasted.claim_delights(now, true, Some(("invented".into(), 3))).practice);
}
