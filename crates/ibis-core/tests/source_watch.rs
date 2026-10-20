//! The real platform watcher, using synthetic files only.
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use ibis_core::model::LoadResult;
use ibis_core::source::{SourceError, SourceWatch};

fn next(rx: &Receiver<Result<LoadResult, SourceError>>) -> Result<LoadResult, SourceError> {
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
    assert_eq!(initial.references.len(), 1);
    std::fs::write(&path, "@misc{partial, title={Unclosed}").unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(next(&rx), Err(SourceError::Incomplete)));
    let replacement = dir.path().join("replacement.tmp");
    std::fs::write(&replacement, "@misc{new, title={New}}").unwrap_or_else(|e| panic!("{e}"));
    // Exercise overwrite on Windows as well as Unix, as an editor save does.
    std::fs::rename(&replacement, &path).unwrap_or_else(|e| panic!("{e}"));
    let loaded = next(&rx).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(loaded.references[0].id.native_key, "new");
    assert_eq!(loaded.references[0].id.source, initial.references[0].id.source);
    std::fs::remove_file(&path).unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(next(&rx), Err(SourceError::Unavailable)));
    std::fs::write(&path, "").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(next(&rx).map(|l| l.references.len()), Ok(0));
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
    let keys: Vec<_> = loaded.references.iter().map(|r| r.id.native_key.as_str()).collect();
    assert_eq!(keys, ["new"]);
    // Native backends can deliver duplicate notifications later. A quiet
    // interval cannot establish their absence, so assert contents instead.
}

#[test]
fn initial_damage_is_tolerated_and_reload_warnings_are_not_blanket_rejected() {
    use ibis_core::source::load_library_file;
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let path = dir.path().join("library.bib");
    std::fs::write(&path, "@misc{good}\n@misc{unfinished,").unwrap_or_else(|e| panic!("{e}"));
    let initial = load_library_file(&path, false).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(initial.references.len(), 1);
    assert_eq!(initial.warnings.len(), 1);
    assert!(matches!(load_library_file(&path, true), Err(SourceError::Incomplete)));
    std::fs::write(&path, "@misc{good}\n@misc{good}\n@misc{closed, title=}").unwrap_or_else(|e| panic!("{e}"));
    let reload = load_library_file(&path, true).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(reload.references.len(), 1);
    assert_eq!(reload.warnings.len(), 2);
    std::fs::write(&path, "% Deliberately empty library\n").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(load_library_file(&path, true).map(|l| l.references.len()), Ok(0));
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
        next(&rx).map(|l| l.references[0].id.native_key.clone()),
        Ok("first".into())
    );
    std::fs::write(&first, "@misc{first_updated}").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        next(&rx).map(|l| l.references[0].id.native_key.clone()),
        Ok("first_updated".into())
    );
    std::fs::remove_file(&selected).unwrap_or_else(|e| panic!("{e}"));
    symlink(&second, &selected).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        next(&rx).map(|l| l.references[0].id.native_key.clone()),
        Ok("second".into())
    );
    std::fs::write(&first, "@misc{obsolete}").unwrap_or_else(|e| panic!("{e}"));
    assert!(rx.recv_timeout(Duration::from_millis(800)).is_err());
    std::fs::write(&second, "@misc{second_updated}").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        next(&rx).map(|l| l.references[0].id.native_key.clone()),
        Ok("second_updated".into())
    );
    std::fs::remove_file(&second).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(next(&rx).err(), Some(SourceError::Unavailable));
    std::fs::write(&second, "@misc{restored}").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        next(&rx).map(|l| l.references[0].id.native_key.clone()),
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
    assert_eq!(last_good.references[0].id.native_key, "first");
    std::fs::remove_file(&selected).unwrap_or_else(|e| panic!("{e}"));
    // Relative targets resolve from the symlink's parent, rather than the process cwd.
    symlink("../second/target.bib", &selected).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(next(&rx).err(), Some(SourceError::Unavailable));
    assert_eq!(last_good.references[0].id.native_key, "first");
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
    assert_eq!(recovered.references[0].id.native_key, "second");
}

#[cfg(windows)]
#[test]
fn verbatim_leaf_names_and_ordinary_aliases_watch_the_selected_file() {
    for suffix in [".", " "] {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let parent = std::fs::canonicalize(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        let ordinary = dunce::simplified(&parent).join("library.bib");
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
            assert_eq!(initial.references[0].id.native_key, key);
            std::fs::write(&selected, format!("@misc{{{key}_updated}}")).unwrap_or_else(|e| panic!("{e}"));
            let loaded = next(&rx).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(loaded.references[0].id.native_key, format!("{key}_updated"));
            assert_eq!(loaded.references[0].id.source, initial.references[0].id.source);
        }
    }
}

#[test]
fn unchanged_damaged_initial_load_does_not_publish_a_strict_reload() {
    use ibis_core::source::load_library_snapshot;
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let path = dir.path().join("library.bib");
    std::fs::write(&path, "@misc{good}\n@misc{unfinished,").unwrap_or_else(|e| panic!("{e}"));
    let (initial, snapshot) = load_library_snapshot(&path, false).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(initial.references.len(), 1);
    assert_eq!(initial.warnings.len(), 1);
    let (tx, rx) = mpsc::channel();
    let _watch = SourceWatch::with_snapshot(&path, snapshot, move |result| {
        let _ = tx.send(result);
    })
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(rx.recv_timeout(Duration::from_millis(900)).is_err());
    std::fs::write(&path, "@misc{updated}").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        next(&rx).map(|l| l.references[0].id.native_key.clone()),
        Ok("updated".into())
    );
}

#[test]
fn changes_before_watch_registration_are_still_loaded_even_at_the_same_size() {
    use ibis_core::source::load_library_snapshot;
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let path = dir.path().join("library.bib");
    std::fs::write(&path, "@misc{old}").unwrap_or_else(|e| panic!("{e}"));
    let (_, snapshot) = load_library_snapshot(&path, false).unwrap_or_else(|e| panic!("{e}"));
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
        next(&rx).map(|l| l.references[0].id.native_key.clone()),
        Ok("new".into())
    );
}

#[test]
fn unchanged_contents_after_a_gap_error_publish_recovery() {
    use ibis_core::source::load_library_snapshot;
    for contents in ["@misc{same}", "@misc{same}\n@misc{unfinished,"] {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = dir.path().join("library.bib");
        std::fs::write(&path, contents).unwrap_or_else(|e| panic!("{e}"));
        let (initial, snapshot) = load_library_snapshot(&path, false).unwrap_or_else(|e| panic!("{e}"));
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
        assert_eq!(recovered.references[0].id.native_key, "same");
        assert_eq!(recovered.warnings.len(), initial.warnings.len());
    }
}

#[test]
fn damaged_changes_in_the_registration_gap_still_use_strict_reloads() {
    use ibis_core::source::load_library_snapshot;
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let path = dir.path().join("library.bib");
    std::fs::write(&path, "@misc{old}").unwrap_or_else(|e| panic!("{e}"));
    let (initial, snapshot) = load_library_snapshot(&path, false).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(&path, "@misc{changed}\n@misc{unfinished,").unwrap_or_else(|e| panic!("{e}"));
    let (tx, rx) = mpsc::channel();
    let _watch = SourceWatch::with_snapshot(&path, snapshot, move |result| {
        let _ = tx.send(result);
    })
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(next(&rx).err(), Some(SourceError::Incomplete));
    assert_eq!(initial.references[0].id.native_key, "old");
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
    assert_eq!(last_good.references[0].id.native_key, "old");
    std::fs::write(&path, "@misc{new}").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        next(&rx).map(|l| l.references[0].id.native_key.clone()),
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
    let last_good = ibis_core::source::load_library_file(&path, false).unwrap_or_else(|e| panic!("{e}"));
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
    assert_eq!(last_good.references[0].id.native_key, "old");
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
                assert_eq!(loaded.map(|l| l.references[0].id.native_key.clone()), Ok("new".into()));
                break;
            }
        }
    }
}
