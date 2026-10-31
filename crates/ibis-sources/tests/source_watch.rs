//! The real platform watcher, using synthetic files only.
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use ibis_core::contract::{LibraryObject, Record};
use ibis_sources::LoadedLibrary;
fn records(library: &LoadedLibrary) -> Vec<&Record> {
    library
        .snapshot
        .objects()
        .iter()
        .filter_map(|item| match &item.object {
            LibraryObject::Record(record) => Some(record),
            _ => None,
        })
        .collect()
}
use ibis_sources::{SourceError, SourceWatch};

fn next(rx: &Receiver<Result<LoadedLibrary, SourceError>>) -> Result<LoadedLibrary, SourceError> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .unwrap_or_else(|| panic!("watcher did not settle before the deadline"));
        match rx
            .recv_timeout(remaining)
            .unwrap_or_else(|e| panic!("watcher did not report: {e}"))
        {
            Err(SourceError::Changing) => continue,
            result => return result,
        }
    }
}

#[test]
fn settling_wait_ignores_the_watchers_transient_changing_notification() {
    let (tx, rx) = mpsc::channel();
    tx.send(Err(SourceError::Changing)).unwrap_or_else(|e| panic!("{e}"));
    tx.send(Err(SourceError::Unavailable)).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(next(&rx).err(), Some(SourceError::Unavailable));
}

#[test]
fn seeded_watches_reuse_bibtex_and_json_records_in_the_registration_gap() {
    for extension in ["bib", "json"] {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = dir.path().join(format!("synthetic.{extension}"));
        let write = |title: &str| {
            let text = if extension == "bib" {
                format!("@book{{a,title={{Keep}}}}\n@book{{b,title={{{title}}}}}\n")
            } else {
                serde_json::json!([{"id":"a","type":"book","title":"Keep"},{"id":"b","type":"book","title":title}])
                    .to_string()
            };
            std::fs::write(&path, text).unwrap_or_else(|error| panic!("{error}"));
        };
        write("Before");
        let (loaded, observation) = ibis_sources::load_file_snapshot(&path).unwrap_or_else(|error| panic!("{error}"));
        let original = loaded.snapshot;
        write("After");
        let (tx, rx) = mpsc::channel();
        let _watch = SourceWatch::with_library(&path, Some(observation), original.clone(), move |result| {
            let installed = result.as_ref().ok().map(|loaded| loaded.snapshot.clone());
            let _ = tx.send(result);
            installed
        })
        .unwrap_or_else(|error| panic!("{error}"));
        let refreshed = next(&rx).unwrap_or_else(|error| panic!("{error}"));
        assert!(records(&refreshed).iter().any(|record| record.csl["title"] == "After"));
        assert!(std::sync::Arc::ptr_eq(
            &original.objects()[0],
            &refreshed.snapshot.objects()[0]
        ));
        assert!(!std::sync::Arc::ptr_eq(
            &original.objects()[1],
            &refreshed.snapshot.objects()[1]
        ));
    }
}

#[test]
fn interrupted_writes_replacements_deletion_and_empty_files() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let path = dir.path().join("références synthetic.bib");
    std::fs::write(&path, "@misc{old, title={Old}}").unwrap_or_else(|e| panic!("{e}"));
    let (tx, rx) = mpsc::channel();
    let _watch = SourceWatch::new(&path, move |result| {
        let _ = tx.send(result);
    })
    .unwrap_or_else(|e| panic!("{e}"));
    let initial = next(&rx).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(records(&initial).len(), 1);
    std::fs::write(&path, "@misc{partial, title={Unclosed}").unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(next(&rx), Err(SourceError::Incomplete)));
    let replacement = dir.path().join("replacement.tmp");
    std::fs::write(&replacement, "@misc{new, title={New}}").unwrap_or_else(|e| panic!("{e}"));
    // Exercise overwrite on Windows as well as Unix, as an editor save does.
    std::fs::rename(&replacement, &path).unwrap_or_else(|e| panic!("{e}"));
    let loaded = next(&rx).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(records(&loaded)[0].citation_keys[0].value, "new");
    assert_eq!(records(&loaded)[0].id.library, records(&initial)[0].id.library);
    std::fs::remove_file(&path).unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(next(&rx), Err(SourceError::Unavailable)));
    std::fs::write(&path, "").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(next(&rx).map(|l| records(&l).len()), Ok(0));
    std::fs::write(&path, [0xff]).unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(next(&rx), Err(SourceError::Unreadable)));
}

#[test]
fn change_bursts_publish_only_the_settled_contents() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let path = dir.path().join("library.bib");
    std::fs::write(&path, "@misc{old}").unwrap_or_else(|e| panic!("{e}"));
    let (tx, rx) = mpsc::channel();
    let (resume, paused) = mpsc::channel();
    let mut first_load = Some(paused);
    let _watch = SourceWatch::new(&path, move |result| {
        let loaded = result.is_ok();
        let _ = tx.send(result);
        // Hold the worker inside its first successful callback. File events
        // still arrive, but no intermediate write can be read or published,
        // even if this test thread is descheduled between writes.
        if loaded && let Some(paused) = first_load.take() {
            let _ = paused.recv();
        }
    })
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(next(&rx).is_ok());
    for contents in ["", "@misc{partial,", "@misc{new}"] {
        std::fs::write(&path, contents).unwrap_or_else(|e| panic!("{e}"));
    }
    resume.send(()).unwrap_or_else(|e| panic!("{e}"));
    let loaded = next(&rx).unwrap_or_else(|e| panic!("{e}"));
    let keys: Vec<_> = records(&loaded)
        .iter()
        .map(|r| r.citation_keys[0].value.as_str())
        .collect();
    assert_eq!(keys, ["new"]);
    // Native backends can deliver duplicate notifications later. A quiet
    // interval cannot establish their absence, so assert contents instead.
}

#[test]
fn initial_damage_is_tolerated_and_reload_warnings_are_not_blanket_rejected() {
    use ibis_sources::{load_file, reload_file};
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let path = dir.path().join("library.bib");
    std::fs::write(&path, "@misc{good}\n@misc{unfinished,").unwrap_or_else(|e| panic!("{e}"));
    let initial = load_file(&path).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(records(&initial).len(), 1);
    assert_eq!(initial.warnings.len(), 1);
    assert!(matches!(reload_file(&path), Err(SourceError::Incomplete)));
    std::fs::write(
        &path,
        "@misc{good}\n@misc{good}\n@misc{retained}\n@misc{closed, title=}",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let reload = reload_file(&path).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(records(&reload).len(), 1);
    assert_eq!(records(&reload)[0].citation_key(), Ok(Some("retained")));
    assert_eq!(reload.warnings.len(), 2);
    std::fs::write(&path, "% Deliberately empty library\n").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(reload_file(&path).map(|l| records(&l).len()), Ok(0));
}

#[cfg(unix)]
#[test]
fn symlink_target_changes_and_retargeting_keep_reloading() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    for name in ["selected", "first", "second"] {
        std::fs::create_dir(dir.path().join(name)).unwrap_or_else(|e| panic!("{e}"));
    }
    let selected = dir.path().join("selected/library.bib");
    let first = dir.path().join("first/target.bib");
    let second = dir.path().join("second/target.bib");
    std::fs::write(&first, "@misc{first}").unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(&second, "@misc{second}").unwrap_or_else(|e| panic!("{e}"));
    symlink(&first, &selected).unwrap_or_else(|e| panic!("{e}"));
    let (tx, rx) = mpsc::channel();
    let _watch = SourceWatch::new(&selected, move |result| {
        let _ = tx.send(result);
    })
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        next(&rx).map(|l| records(&l)[0].citation_keys[0].value.clone()),
        Ok("first".into())
    );
    std::fs::write(&first, "@misc{first_updated}").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        next(&rx).map(|l| records(&l)[0].citation_keys[0].value.clone()),
        Ok("first_updated".into())
    );
    std::fs::remove_file(&selected).unwrap_or_else(|e| panic!("{e}"));
    symlink(&second, &selected).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        next(&rx).map(|l| records(&l)[0].citation_keys[0].value.clone()),
        Ok("second".into())
    );
    std::fs::write(&first, "@misc{obsolete}").unwrap_or_else(|e| panic!("{e}"));
    assert!(rx.recv_timeout(Duration::from_millis(800)).is_err());
    std::fs::write(&second, "@misc{second_updated}").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        next(&rx).map(|l| records(&l)[0].citation_keys[0].value.clone()),
        Ok("second_updated".into())
    );
    std::fs::remove_file(&second).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(next(&rx).err(), Some(SourceError::Unavailable));
    std::fs::write(&second, "@misc{restored}").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        next(&rx).map(|l| records(&l)[0].citation_keys[0].value.clone()),
        Ok("restored".into())
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_retargeted_to_a_missing_file_recovers_when_the_file_appears() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    for name in ["selected", "first", "second"] {
        std::fs::create_dir(dir.path().join(name)).unwrap_or_else(|e| panic!("{e}"));
    }
    let selected = dir.path().join("selected/library.bib");
    let first = dir.path().join("first/target.bib");
    let second = dir.path().join("second/target.bib");
    std::fs::write(&first, "@misc{first}").unwrap_or_else(|e| panic!("{e}"));
    symlink(&first, &selected).unwrap_or_else(|e| panic!("{e}"));
    let (tx, rx) = mpsc::channel();
    let _watch = SourceWatch::new(&selected, move |result| {
        let _ = tx.send(result);
    })
    .unwrap_or_else(|e| panic!("{e}"));
    let last_good = next(&rx).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(records(&last_good)[0].citation_keys[0].value, "first");
    std::fs::remove_file(&selected).unwrap_or_else(|e| panic!("{e}"));
    // Relative targets resolve from the symlink's parent, rather than the process cwd.
    symlink("../second/target.bib", &selected).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(next(&rx).err(), Some(SourceError::Unavailable));
    assert_eq!(records(&last_good)[0].citation_keys[0].value, "first");
    std::fs::write(&first, "@misc{obsolete}").unwrap_or_else(|e| panic!("{e}"));
    // Retrying the missing target is allowed; the obsolete target must never
    // become a successful candidate again.
    let deadline = Instant::now() + Duration::from_millis(800);
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(remaining) {
            Ok(result) => assert_eq!(result.err(), Some(SourceError::Unavailable)),
            Err(mpsc::RecvTimeoutError::Timeout) => break,
            Err(error) => panic!("{error}"),
        }
    }
    std::fs::write(&second, "@misc{second}").unwrap_or_else(|e| panic!("{e}"));
    let recovered = next(&rx).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(records(&recovered)[0].citation_keys[0].value, "second");
}

#[cfg(windows)]
#[test]
fn verbatim_leaf_names_and_ordinary_aliases_watch_the_selected_file() {
    for suffix in [".", " "] {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let parent = std::fs::canonicalize(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        let ordinary = dir.path().join("library.bib");
        let verbatim = parent.join(format!("library.bib{suffix}"));
        std::fs::write(&ordinary, "@misc{ordinary}").unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(&verbatim, "@misc{verbatim}").unwrap_or_else(|e| panic!("{e}"));
        for (selected, key) in [
            (verbatim.clone(), "verbatim"),
            (ordinary.with_file_name(format!("library.bib{suffix}")), "ordinary"),
        ] {
            let (tx, rx) = mpsc::channel();
            let _watch = SourceWatch::new(&selected, move |result| {
                let _ = tx.send(result);
            })
            .unwrap_or_else(|e| panic!("{e}"));
            let initial = next(&rx).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(records(&initial)[0].citation_keys[0].value, key);
            std::fs::write(&selected, format!("@misc{{{key}_updated}}")).unwrap_or_else(|e| panic!("{e}"));
            let loaded = next(&rx).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(records(&loaded)[0].citation_keys[0].value, format!("{key}_updated"));
            assert_eq!(records(&loaded)[0].id.library, records(&initial)[0].id.library);
        }
    }
}

#[test]
fn unchanged_damaged_initial_load_does_not_publish_a_strict_reload() {
    use ibis_sources::load_file_snapshot;
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let path = dir.path().join("library.bib");
    std::fs::write(&path, "@misc{good}\n@misc{unfinished,").unwrap_or_else(|e| panic!("{e}"));
    let (initial, snapshot) = load_file_snapshot(&path).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(records(&initial).len(), 1);
    assert_eq!(initial.warnings.len(), 1);
    let (tx, rx) = mpsc::channel();
    let _watch = SourceWatch::with_snapshot(&path, snapshot, move |result| {
        let _ = tx.send(result);
    })
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(rx.recv_timeout(Duration::from_millis(900)).is_err());
    std::fs::write(&path, "@misc{updated}").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        next(&rx).map(|l| records(&l)[0].citation_keys[0].value.clone()),
        Ok("updated".into())
    );
}

#[test]
fn changes_before_watch_registration_are_still_loaded_even_at_the_same_size() {
    use ibis_sources::load_file_snapshot;
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let path = dir.path().join("library.bib");
    std::fs::write(&path, "@misc{old}").unwrap_or_else(|e| panic!("{e}"));
    let (_, snapshot) = load_file_snapshot(&path).unwrap_or_else(|e| panic!("{e}"));
    let modified = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(&path, "@misc{new}").unwrap_or_else(|e| panic!("{e}"));
    std::fs::File::options()
        .write(true)
        .open(&path)
        .and_then(|f| f.set_modified(modified))
        .unwrap_or_else(|e| panic!("{e}"));
    let (tx, rx) = mpsc::channel();
    let _watch = SourceWatch::with_snapshot(&path, snapshot, move |result| {
        let _ = tx.send(result);
    })
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        next(&rx).map(|l| records(&l)[0].citation_keys[0].value.clone()),
        Ok("new".into())
    );
}

#[test]
fn unchanged_contents_after_a_gap_error_publish_recovery() {
    use ibis_sources::load_file_snapshot;
    for contents in ["@misc{same}", "@misc{same}\n@misc{unfinished,"] {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = dir.path().join("library.bib");
        std::fs::write(&path, contents).unwrap_or_else(|e| panic!("{e}"));
        let (initial, snapshot) = load_file_snapshot(&path).unwrap_or_else(|e| panic!("{e}"));
        std::fs::remove_file(&path).unwrap_or_else(|e| panic!("{e}"));
        let (tx, rx) = mpsc::channel();
        let _watch = SourceWatch::with_snapshot(&path, snapshot, move |result| {
            let _ = tx.send(result);
        })
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(next(&rx).err(), Some(SourceError::Unavailable));
        // Repeated unavailable reads must preserve the initial observation,
        // including its tolerant damaged-data policy, until recovery.
        assert_eq!(next(&rx).err(), Some(SourceError::Unavailable));
        std::fs::write(&path, contents).unwrap_or_else(|e| panic!("{e}"));
        let recovered = next(&rx).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(records(&recovered)[0].citation_keys[0].value, "same");
        assert_eq!(recovered.warnings.len(), initial.warnings.len());
    }
}

#[test]
fn damaged_changes_in_the_registration_gap_still_use_strict_reloads() {
    use ibis_sources::load_file_snapshot;
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let path = dir.path().join("library.bib");
    std::fs::write(&path, "@misc{old}").unwrap_or_else(|e| panic!("{e}"));
    let (initial, snapshot) = load_file_snapshot(&path).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(&path, "@misc{changed}\n@misc{unfinished,").unwrap_or_else(|e| panic!("{e}"));
    let (tx, rx) = mpsc::channel();
    let _watch = SourceWatch::with_snapshot(&path, snapshot, move |result| {
        let _ = tx.send(result);
    })
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(next(&rx).err(), Some(SourceError::Incomplete));
    assert_eq!(records(&initial)[0].citation_keys[0].value, "old");
}

#[test]
fn unavailable_reads_retry_without_another_file_event() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let path = dir.path().join("library.bib");
    std::fs::write(&path, "@misc{old}").unwrap_or_else(|e| panic!("{e}"));
    let (tx, rx) = mpsc::channel();
    let watch = SourceWatch::new(&path, move |result| {
        let _ = tx.send(result);
    })
    .unwrap_or_else(|e| panic!("{e}"));
    let last_good = next(&rx).unwrap_or_else(|e| panic!("{e}"));
    std::fs::remove_file(&path).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(next(&rx).err(), Some(SourceError::Unavailable));
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(2)).ok().and_then(Result::err),
        Some(SourceError::Unavailable)
    );
    assert_eq!(records(&last_good)[0].citation_keys[0].value, "old");
    std::fs::write(&path, "@misc{new}").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        next(&rx).map(|l| records(&l)[0].citation_keys[0].value.clone()),
        Ok("new".into())
    );
    assert!(rx.recv_timeout(Duration::from_millis(800)).is_err());
    std::fs::remove_file(&path).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(next(&rx).err(), Some(SourceError::Unavailable));
    drop(watch);
    assert!(rx.recv_timeout(Duration::from_millis(900)).is_err());
}

#[cfg(windows)]
#[test]
fn a_long_exclusive_lock_recovers_without_a_followup_save() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let path = dir.path().join("library.bib");
    std::fs::write(&path, "@misc{old}").unwrap_or_else(|e| panic!("{e}"));
    let last_good = ibis_sources::load_file(&path).unwrap_or_else(|e| panic!("{e}"));
    // Finish the edit before watching, then hold a read-only exclusive handle.
    // Registering while locked forces the initial check to observe Unavailable:
    // Windows can defer a writer's change notification until its handle closes.
    std::fs::write(&path, "@misc{new}").unwrap_or_else(|e| panic!("{e}"));
    let locked = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&path)
        .unwrap_or_else(|e| panic!("{e}"));
    let (tx, rx) = mpsc::channel();
    let _watch = SourceWatch::new(&path, move |result| {
        let _ = tx.send(result);
    })
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(next(&rx).err(), Some(SourceError::Unavailable));
    assert_eq!(records(&last_good)[0].citation_keys[0].value, "old");
    std::thread::sleep(Duration::from_secs(2));
    // Closing this read-only handle does not save or modify the source.
    drop(locked);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .unwrap_or_else(|| panic!("lock did not recover"));
        match rx.recv_timeout(remaining).unwrap_or_else(|e| panic!("{e}")) {
            Err(SourceError::Unavailable | SourceError::Changing) => continue,
            loaded => {
                assert_eq!(
                    loaded.map(|l| records(&l)[0].citation_keys[0].value.clone()),
                    Ok("new".into())
                );
                break;
            }
        }
    }
}

#[test]
fn json_reloads_retain_failed_snapshot_and_accept_stable_empty_array() {
    let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let path = dir.path().join("synthetic.json");
    std::fs::write(&path, r#"[{"id":"old","type":"book"}]"#).unwrap_or_else(|error| panic!("{error}"));
    let (tx, rx) = mpsc::channel();
    let _watch = SourceWatch::new(&path, move |result| {
        let _ = tx.send(result);
    })
    .unwrap_or_else(|error| panic!("{error}"));
    let first = next(&rx).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(records(&first).len(), 1);
    std::fs::write(&path, "[{\"id\":").unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(next(&rx).err(), Some(SourceError::Incomplete));
    assert_eq!(records(&first).len(), 1);
    let replacement = dir.path().join("replacement.tmp");
    std::fs::write(&replacement, r#"[{"id":"new","type":"book"}]"#).unwrap_or_else(|error| panic!("{error}"));
    std::fs::rename(replacement, &path).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        records(&next(&rx).unwrap_or_else(|error| panic!("{error}")))[0].citation_key(),
        Ok(Some("new"))
    );
    std::fs::write(&path, "[]").unwrap_or_else(|error| panic!("{error}"));
    assert!(records(&next(&rx).unwrap_or_else(|error| panic!("{error}"))).is_empty());
}

#[test]
fn private_location_changes_publish_a_new_owner_even_if_public_hashes_match() {
    use ibis_sources::load_file_snapshot;
    let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let path = dir.path().join("synthetic.bib");
    std::fs::write(&path, "@book{one,file={one.pdf}}").unwrap_or_else(|error| panic!("{error}"));
    let (initial, observation) = load_file_snapshot(&path).unwrap_or_else(|error| panic!("{error}"));
    let (tx, rx) = mpsc::channel();
    let _watch = SourceWatch::with_snapshot(&path, observation, move |result| {
        let _ = tx.send(result);
    })
    .unwrap_or_else(|error| panic!("{error}"));
    std::fs::write(&path, "@book{one,file={two.pdf}}").unwrap_or_else(|error| panic!("{error}"));
    let changed = next(&rx).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(initial.snapshot.fingerprints(), changed.snapshot.fingerprints());
    assert_eq!(changed.local_attachment_count(), 1);
}

#[test]
fn long_identity_attachment_limits_keep_last_good_owner_until_recovery() {
    let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let path = dir.path().join("synthetic.json");
    std::fs::write(&path, r#"[{"id":"initial","type":"book"}]"#).unwrap_or_else(|error| panic!("{error}"));
    let (initial, observation) = ibis_sources::load_file_snapshot(&path).unwrap_or_else(|error| panic!("{error}"));
    let (tx, rx) = mpsc::channel();
    let _watch = SourceWatch::with_snapshot(&path, observation, move |candidate| {
        let _ = tx.send(candidate);
    })
    .unwrap_or_else(|error| panic!("{error}"));
    let limited = serde_json::json!([{"id":"x".repeat(1024),"citation-key":"long","type":"book","file":"https://example.invalid/x;".repeat(256)}]);
    std::fs::write(
        &path,
        serde_json::to_vec(&limited).unwrap_or_else(|error| panic!("{error}")),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(next(&rx).err(), Some(SourceError::LoadLimit));
    assert_eq!(records(&initial)[0].citation_keys[0].value, "initial");
    std::fs::write(&path, r#"[{"id":"recovered","type":"book"}]"#).unwrap_or_else(|error| panic!("{error}"));
    let recovered = next(&rx).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(records(&recovered)[0].citation_keys[0].value, "recovered");
}
