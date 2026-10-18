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
    let _watch = SourceWatch::new(&path, move |result| {
        let _ = tx.send(result);
    })
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(next(&rx).is_ok());
    for contents in ["", "@misc{partial,", "@misc{new}"] {
        std::fs::write(&path, contents).unwrap_or_else(|e| panic!("{e}"));
        std::thread::sleep(Duration::from_millis(30));
    }
    let loaded = next(&rx).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(loaded.references[0].id.native_key, "new");
    assert!(rx.recv_timeout(Duration::from_millis(800)).is_err());
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
    assert!(rx.recv_timeout(Duration::from_millis(800)).is_err());
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
