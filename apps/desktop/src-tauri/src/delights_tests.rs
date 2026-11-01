#![allow(clippy::unwrap_used)]
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
    let saved = ibis_core::settings::load(&path).settings;
    assert_eq!(saved.small_surprises, Some(false));
    assert_eq!(saved.delights.unwrap().paste_count, 1);
}

#[test]
fn facts_follow_five_clicks_and_reopen_preserves_the_sequence_after_failed_saves() {
    let dir = tempfile::tempdir().unwrap();
    let file = SettingsFile::new(dir.path().join("settings.json"));
    let now = moment(2026, 11, 1);
    for fact in [
        DelightFact::Thoth,
        DelightFact::Touch,
        DelightFact::Scarlet,
        DelightFact::Mummies,
        DelightFact::National,
        DelightFact::Flight,
    ] {
        for _ in 0..4 {
            file.claim_about(now, true);
        }
        assert_eq!(file.claim_about(now, true).fact, Some(fact));
    }
    file.flush_delights().unwrap();
    let reopened = SettingsFile::new(file.path.clone());
    for _ in 0..4 {
        reopened.claim_about(now, true);
    }
    assert_eq!(reopened.claim_about(now, true).fact, Some(DelightFact::Thoth));
}

#[test]
fn first_stable_birthday_has_one_build_time_anchor_and_one_day() {
    assert!(!release_birthday(moment(2028, 11, 1), None));
    assert!(!release_birthday(moment(2027, 11, 1), Some([2026, 11, 1])));
    assert!(release_birthday(moment(2028, 11, 1), Some([2026, 11, 1])));
    assert!(!release_birthday(moment(2028, 11, 2), Some([2026, 11, 1])));
    assert!(!release_birthday(moment(2028, 11, 1), Some([i32::MAX, 11, 1])));
}

#[test]
fn only_current_successful_real_loads_can_claim_library_feedback() {
    use crate::library::engine_file::FileLibrary;
    use crate::library::{Library, Source};
    use std::sync::Arc;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invented.bib");
    std::fs::write(&path, "@book{invented,title={Invented Research},year={2026}}").unwrap();
    let mut library = Library::default();
    library.file = Some(Arc::new(FileLibrary::open(&path).unwrap()));
    library.source = Some(Source::File(path));
    let loaded = crate::library::snapshot_status(&library);
    assert!(library_claim(&loaded, Some(&loaded.revision)).is_some());
    let mut stable = loaded.clone();
    stable.source_counts = std::collections::BTreeMap::from([("invented.bib".into(), 1)]);
    assert_eq!(
        library_claim(&stable, Some(&stable.revision)).unwrap().0,
        "5df35b7d63cc14e3"
    );
    assert!(library_claim(&loaded, Some("retired-revision")).is_none());
    for state in [
        crate::library::LibraryState::Loading,
        crate::library::LibraryState::Reloading,
        crate::library::LibraryState::Error,
        crate::library::LibraryState::None,
    ] {
        let mut refused = loaded.clone();
        refused.state = state;
        assert!(library_claim(&refused, Some(&loaded.revision)).is_none());
    }
    let mut sample = loaded.clone();
    sample.sample = true;
    assert!(library_claim(&sample, Some(&loaded.revision)).is_none());
    assert!(
        quiet_load(&sample, Some(&loaded.revision)),
        "samples still qualify for paste tips and the welcome mark"
    );
    let mut stale = loaded.clone();
    stale.stale = true;
    assert!(library_claim(&stale, Some(&loaded.revision)).is_none());
    let mut failed_write = loaded.clone();
    failed_write.note = Some("Invented save failure".into());
    assert!(library_claim(&failed_write, Some(&loaded.revision)).is_none());
}

#[test]
fn simultaneous_windows_share_one_claim_and_paste_never_waits_for_the_writer() {
    use std::sync::{Arc, Barrier, mpsc};
    use std::time::Duration;
    let dir = tempfile::tempdir().unwrap();
    let settings = SettingsFile::new(dir.path().join("settings.json"));
    let barrier = Arc::new(Barrier::new(2));
    let windows: Vec<_> = (0..2)
        .map(|_| {
            let settings = settings.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                settings.claim_delights(moment(2026, 11, 1), true, Some(("invented".into(), 100)))
            })
        })
        .collect();
    let answers: Vec<_> = windows.into_iter().map(|window| window.join().unwrap()).collect();
    assert_eq!(answers.iter().filter(|answer| answer.peck).count(), 1);
    assert_eq!(answers.iter().filter(|answer| answer.status.is_some()).count(), 1);
    let (entered, held) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let writer = settings.clone();
    let writing = std::thread::spawn(move || {
        writer.update(|_| {
            entered.send(()).unwrap();
            released.recv_timeout(Duration::from_secs(2)).unwrap();
        })
    });
    held.recv_timeout(Duration::from_secs(1)).unwrap();
    let (pasted, outcome) = mpsc::channel();
    let copy = settings.clone();
    let paste = std::thread::spawn(move || pasted.send(copy.record_delight_paste(1)).unwrap());
    let result = outcome.recv_timeout(Duration::from_millis(500));
    release.send(()).unwrap();
    writing.join().unwrap().unwrap();
    paste.join().unwrap();
    assert_eq!(result.unwrap(), Some(1));
}

fn moment(year: i32, month: u8, day: u8) -> LocalMoment {
    LocalMoment { year, month, day }
}

#[test]
fn edited_calendar_metadata_cannot_panic_or_block_paste() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(
        &path,
        r#"{"schema_version":2,"delights":{"first_run":[2147483647,11,1]}}"#,
    )
    .unwrap();
    let settings = SettingsFile::new(path);
    let view = settings.claim_delights(moment(2026, 11, 1), true, None);
    assert!(!view.anniversary);
    assert_eq!(view.status, None);
    assert_eq!(settings.record_delight_paste(1), Some(1));
}

#[test]
fn a_large_import_keeps_every_milestone_and_first_paste_tip_takes_priority() {
    let dir = tempfile::tempdir().unwrap();
    let settings = SettingsFile::new(dir.path().join("settings.json"));
    let now = moment(2026, 11, 1);
    let library = || Some(("invented".into(), 10000));
    assert_eq!(
        settings.claim_delights(now, true, library()).status,
        Some(DelightStatus::Library100)
    );
    settings.record_delight_paste(1);
    assert_eq!(
        settings.claim_delights(now, true, library()).status,
        Some(DelightStatus::PasteTip)
    );
    assert_eq!(
        settings.claim_delights(now, true, library()).status,
        Some(DelightStatus::Library1000)
    );
    assert_eq!(
        settings.claim_delights(now, true, library()).status,
        Some(DelightStatus::Library10000)
    );
    assert_eq!(settings.claim_delights(now, true, library()).status, None);
}

#[test]
fn stale_claims_cannot_consume_a_newer_presentations_feedback() {
    let dir = tempfile::tempdir().unwrap();
    let settings = SettingsFile::new(dir.path().join("settings.json"));
    let now = moment(2026, 11, 1);
    let stale = settings.claim_delights_when(now, true, Some(("invented".into(), 100)), || false, false);
    assert!(!stale.peck);
    assert_eq!(stale.status, None);
    for (count, status) in [
        (100, DelightStatus::Library100),
        (1000, DelightStatus::Library1000),
        (10000, DelightStatus::Library10000),
    ] {
        assert_eq!(
            settings
                .claim_delights(now, true, Some(("invented".into(), count)))
                .status,
            Some(status)
        );
        assert_eq!(
            settings
                .claim_delights(now, true, Some(("invented".into(), count)))
                .status,
            None
        );
    }
    assert_eq!(settings.record_delight_paste(2), Some(1));
    assert_eq!(
        settings.record_delight_paste(1),
        None,
        "a delayed unique delivery is counted, without a second milestone"
    );
    settings.flush_delights().unwrap();
    let reopened = SettingsFile::new(settings.path.clone());
    assert_eq!(reopened.record_delight_paste(3), None);
    assert_eq!(
        reopened.delights.lock().unwrap().record.as_ref().unwrap().paste_count,
        3
    );
    assert_eq!(
        reopened
            .claim_delights(now, true, Some(("invented".into(), 10000)))
            .status,
        Some(DelightStatus::PasteTip)
    );
    assert_eq!(
        reopened
            .claim_delights(now, true, Some(("invented".into(), 10000)))
            .status,
        None
    );
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
    assert_eq!(first.status, Some(DelightStatus::Library100));
    assert!(blocked.flush_delights().is_err());
    let again = blocked.claim_delights(now, true, Some(("invented".into(), 1000)));
    assert!(!again.peck);
    assert_eq!(again.status, Some(DelightStatus::Library1000));
    assert_eq!(
        blocked
            .claim_delights(now, true, Some(("invented".into(), 1000)))
            .status,
        None
    );
    assert_eq!(
        blocked
            .claim_delights(now, true, Some(("invented".into(), 1000)))
            .status,
        None
    );
}

#[test]
fn paste_tip_receipts_and_anniversary_eligibility() {
    let dir = tempfile::tempdir().unwrap();
    let settings = SettingsFile::new(dir.path().join("settings.json"));
    let first = moment(2026, 11, 1);
    assert_eq!(settings.claim_delights(first, true, None).status, None);
    assert_eq!(settings.record_delight_paste(1), Some(1));
    assert_eq!(settings.record_delight_paste(1), None);
    assert_eq!(
        settings.claim_delights(first, true, None).status,
        Some(DelightStatus::PasteTip)
    );
    assert_eq!(settings.claim_delights(first, true, None).status, None);
    for token in 2..=1000 {
        assert_eq!(
            settings.record_delight_paste(token),
            [100, 1000].contains(&token).then_some(token)
        );
    }
    assert!(!settings.claim_delights(moment(2027, 10, 31), false, None).anniversary);
    assert!(settings.claim_delights(moment(2027, 11, 1), false, None).anniversary);
    assert!(!settings.claim_delights(moment(2027, 11, 2), false, None).anniversary);
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
    settings.update(|s| s.larger_text = Some(true)).unwrap();
    let saved = ibis_core::settings::load(&path).settings;
    assert_eq!(saved.shortcut.as_deref(), Some("Alt+Space"));
    assert_eq!(saved.unknown["invented_future"], original["invented_future"]);
    assert!(saved.delights.is_some());
    assert!(
        serde_json::to_value(ibis_core::settings::Settings::default())
            .unwrap()
            .get("delights")
            .is_none()
    );
}
