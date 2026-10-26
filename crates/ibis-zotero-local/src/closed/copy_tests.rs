use super::*;
use crate::closed::tests::{control, edit, fixture};

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic scratch"))
}
fn candidates(parent: &Path) -> usize {
    std::fs::read_dir(parent.join(ROOT))
        .unwrap_or_else(|_| panic!("private root"))
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("copy-"))
        .count()
}

#[test]
fn exactly_three_mutating_attempts_then_refusal_and_cleanup() {
    let source = fixture(10);
    let scratch = scratch();
    let store = Store::open(scratch.path()).unwrap_or_else(|e| panic!("{e}"));
    let mut attempts = 0;
    let outcome = store.read_with(
        source.path(),
        &control(),
        &mut || {
            attempts += 1;
            edit(
                source.path(),
                &format!("UPDATE items SET version={} WHERE itemID=10", attempts + 10),
            );
            Ok(())
        },
        &mut || Ok(()),
    );
    assert_eq!(attempts, 3);
    assert_eq!(outcome.err(), Some(ClosedError::Changing));
    assert_eq!(candidates(scratch.path()), 0);
}

#[test]
fn retries_a_changed_copy_then_reads_the_stable_candidate() {
    let source = fixture(10);
    let scratch = scratch();
    let store = Store::open(scratch.path()).unwrap_or_else(|e| panic!("{e}"));
    let mut attempts = 0;
    let outcome = store.read_with(
        source.path(),
        &control(),
        &mut || {
            attempts += 1;
            if attempts == 1 {
                edit(
                    source.path(),
                    "UPDATE itemDataValues SET value='The Corrected Synthetic Atlas' WHERE valueID=1",
                );
            }
            Ok(())
        },
        &mut || Ok(()),
    );
    assert_eq!(attempts, 2);
    let snapshot = outcome.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        snapshot.libraries[0].items[0]
            .data
            .fields
            .get("title")
            .map(String::as_str),
        Some("The Corrected Synthetic Atlas")
    );
    assert_eq!(candidates(scratch.path()), 0);
}

#[test]
fn process_starting_during_copy_refuses_and_does_not_block_source_writes() {
    let source = fixture(10);
    let scratch = scratch();
    let store = Store::open(scratch.path()).unwrap_or_else(|e| panic!("{e}"));
    let mut checks = 0;
    let result = store.read_with(
        source.path(),
        &control(),
        &mut || {
            let mut source_file = OpenOptions::new()
                .write(true)
                .open(source.path().join("zotero.sqlite"))
                .unwrap_or_else(|_| panic!("writer opens while copied source remains open"));
            source_file.flush().unwrap_or_else(|_| panic!("writer flush"));
            Ok(())
        },
        &mut || {
            checks += 1;
            if checks >= 3 { Err(ClosedError::Running) } else { Ok(()) }
        },
    );
    assert_eq!(result.err(), Some(ClosedError::Running));
    assert_eq!(candidates(scratch.path()), 0);
}

#[test]
fn cancellation_removes_copy_and_process_check_failure_does_not_create_one() {
    let source = fixture(10);
    let scratch = scratch();
    let store = Store::open(scratch.path()).unwrap_or_else(|e| panic!("{e}"));
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let check = cancelled.clone();
    let mut control = control();
    control.cancelled = std::sync::Arc::new(move || check.load(std::sync::atomic::Ordering::SeqCst));
    let result = store.read_with(
        source.path(),
        &control,
        &mut || {
            cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        },
        &mut || Ok(()),
    );
    assert_eq!(result.err(), Some(ClosedError::Cancelled));
    assert_eq!(candidates(scratch.path()), 0);
    let result = store.read_with(
        source.path(),
        &super::super::tests::control(),
        &mut || panic!("must not copy"),
        &mut || Err(ClosedError::ProcessCheck),
    );
    assert_eq!(result.err(), Some(ClosedError::ProcessCheck));
    assert_eq!(candidates(scratch.path()), 0);
}

#[test]
fn startup_cleans_only_marked_private_copies_and_preserves_active_root() {
    let scratch = scratch();
    let root = scratch.path().join(ROOT);
    let store = Store::open(scratch.path()).unwrap_or_else(|e| panic!("{e}"));
    let copy = Copy::new(&root).unwrap_or_else(|e| panic!("{e}"));
    let leftover = copy.path.clone();
    std::mem::forget(copy); // synthetic crash leaves the marked candidate.
    let lookalike = root.join("copy-not-ours");
    std::fs::create_dir(&lookalike).unwrap_or_else(|_| panic!("lookalike"));
    assert_eq!(
        Store::open(scratch.path()).err().map(|_| ClosedError::StorageInUse),
        Some(ClosedError::StorageInUse)
    );
    assert!(leftover.exists());
    drop(store);
    let _store = Store::open(scratch.path()).unwrap_or_else(|e| panic!("{e}"));
    assert!(!leftover.exists());
    assert!(lookalike.exists());
}

#[test]
fn backup_marker_and_owner_only_modes_are_set_before_library_data() {
    let scratch = scratch();
    let store = Store::open(scratch.path()).unwrap_or_else(|e| panic!("{e}"));
    let copy = Copy::new(&store.root).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        std::fs::read(copy.path.join("CACHEDIR.TAG"))
            .unwrap_or_default()
            .starts_with(b"Signature: 8a477f597d28d172789f06886806bc55")
    );
    let file = private_file(&copy.path.join("zotero.sqlite")).unwrap_or_else(|e| panic!("{e}"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            file.metadata().unwrap_or_else(|_| panic!("private file")).mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&copy.path)
                .unwrap_or_else(|_| panic!("private dir"))
                .mode()
                & 0o777,
            0o700
        );
    }
    #[cfg(target_os = "macos")]
    assert_eq!(
        xattr::get(&copy.path, "com.apple.metadata:com_apple_backup_excludeItem").unwrap_or_default(),
        Some(b"com.apple.backupd".to_vec())
    );
    drop(file);
}

#[cfg(unix)]
#[test]
fn source_symlinks_fifo_and_symlinked_private_roots_are_refused() {
    use std::os::unix::fs::symlink;
    let source = fixture(10);
    let scratch = scratch();
    let alias = scratch.path().join(ROOT);
    symlink(source.path(), &alias).unwrap_or_else(|_| panic!("symlink root"));
    assert_eq!(Store::open(scratch.path()).err(), Some(ClosedError::PrivateStorage));
    let scratch2 = self::scratch();
    let store = Store::open(scratch2.path()).unwrap_or_else(|e| panic!("{e}"));
    symlink(
        source.path().join("zotero.sqlite"),
        source.path().join("zotero.sqlite-wal"),
    )
    .unwrap_or_else(|_| panic!("symlink sidecar"));
    assert_eq!(
        store
            .read_with(source.path(), &control(), &mut || Ok(()), &mut || Ok(()))
            .err(),
        Some(ClosedError::UnsafeSource)
    );
    std::fs::remove_file(source.path().join("zotero.sqlite-wal")).unwrap_or_else(|_| panic!("remove synthetic link"));
    let status = std::process::Command::new("mkfifo")
        .arg(source.path().join("zotero.sqlite-wal"))
        .status()
        .unwrap_or_else(|_| panic!("synthetic fifo"));
    assert!(status.success());
    assert_eq!(
        store
            .read_with(source.path(), &control(), &mut || Ok(()), &mut || Ok(()))
            .err(),
        Some(ClosedError::UnsafeSource)
    );
}

#[test]
fn oversized_sources_refuse_before_copying() {
    let source = fixture(10);
    let f = OpenOptions::new()
        .write(true)
        .open(source.path().join("zotero.sqlite"))
        .unwrap_or_else(|_| panic!("synthetic sparse db"));
    f.set_len(MAX_BYTES + 1)
        .unwrap_or_else(|_| panic!("synthetic sparse length"));
    drop(f);
    let scratch = scratch();
    let store = Store::open(scratch.path()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        store
            .read_with(source.path(), &control(), &mut || panic!("must not copy"), &mut || Ok(
                ()
            ))
            .err(),
        Some(ClosedError::TooLarge)
    );
    assert_eq!(candidates(scratch.path()), 0);
}

#[test]
fn reads_committed_wal_only_rows_without_touching_source_database() {
    let source = fixture(10);
    // Preserve bytes while a synthetic connection owns the WAL, then close it
    // and restore those bytes as a crash fixture; production never opens source.
    let db =
        rusqlite::Connection::open(source.path().join("zotero.sqlite")).unwrap_or_else(|_| panic!("synthetic wal"));
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; UPDATE itemDataValues SET value='Committed Synthetic WAL Title' WHERE valueID=1;").unwrap_or_else(|_|panic!("synthetic wal commit"));
    let main = std::fs::read(source.path().join("zotero.sqlite")).unwrap_or_default();
    let wal = std::fs::read(source.path().join("zotero.sqlite-wal")).unwrap_or_default();
    assert!(!wal.is_empty());
    drop(db);
    std::fs::write(source.path().join("zotero.sqlite"), &main).unwrap_or_else(|_| panic!("restore synthetic main"));
    std::fs::write(source.path().join("zotero.sqlite-wal"), &wal).unwrap_or_else(|_| panic!("restore synthetic wal"));
    let snapshot = crate::closed::tests::read_fixture(source.path()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        snapshot.libraries[0].items[0]
            .data
            .fields
            .get("title")
            .map(String::as_str),
        Some("Committed Synthetic WAL Title")
    );
    assert_eq!(
        std::fs::read(source.path().join("zotero.sqlite")).unwrap_or_default(),
        main
    );
    assert_eq!(
        std::fs::read(source.path().join("zotero.sqlite-wal")).unwrap_or_default(),
        wal
    );
    assert!(!source.path().join("zotero.sqlite-shm").exists());
}
