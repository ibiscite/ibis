use super::*;
use crate::zotero_local::closed::tests::{control, edit, fixture};
use std::fs::OpenOptions;

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic scratch"))
}

#[test]
fn an_interrupted_refresh_keeps_the_previous_committed_set() {
    let source = fixture(10);
    let cache = scratch();
    let store = Store::open(cache.path(), &control()).unwrap();
    store
        .read_with(source.path(), &control(), &mut || Ok(()), &mut || Ok(()))
        .unwrap();
    let database = std::fs::read(store.root.join("zotero.sqlite")).unwrap();
    let stamp = std::fs::read(store.root.join(STAMP)).unwrap();
    edit(source.path(), "UPDATE items SET version=99 WHERE itemID=10");
    let result = store.read_with(
        source.path(),
        &control(),
        &mut || Err(ClosedError::Cancelled),
        &mut || Ok(()),
    );
    assert_eq!(result.err(), Some(ClosedError::Cancelled));
    assert_eq!(std::fs::read(store.root.join("zotero.sqlite")).unwrap(), database);
    assert_eq!(std::fs::read(store.root.join(STAMP)).unwrap(), stamp);
    assert_eq!(temporary_files(cache.path()), 0);
}

#[test]
fn cache_without_a_committed_stamp_is_never_read() {
    let source = fixture(10);
    let cache = scratch();
    let store = Store::open(cache.path(), &control()).unwrap();
    let root = cache.path().join(ROOT);
    let mut partial = ibis_core::atomic_file::create_private(&root.join("zotero.sqlite")).unwrap();
    partial.write_all(b"interrupted publication").unwrap();
    drop(partial);
    let mut copies = 0;
    let snapshot = store
        .read_with(
            source.path(),
            &control(),
            &mut || {
                copies += 1;
                Ok(())
            },
            &mut || Ok(()),
        )
        .unwrap();
    assert_eq!(snapshot.libraries[0].items[0].data.key, "SYNTH001");
    assert_eq!(copies, 1);
    assert!(root.join("stamp").is_file());
}

#[cfg(unix)]
#[test]
fn cache_files_and_stamp_are_owner_only() {
    use std::os::unix::fs::MetadataExt;
    let source = fixture(10);
    let cache = scratch();
    let store = Store::open(cache.path(), &control()).unwrap();
    store
        .read_with(source.path(), &control(), &mut || Ok(()), &mut || Ok(()))
        .unwrap();
    let root = cache.path().join(ROOT);
    for name in ["zotero.sqlite", "stamp", ".lock", "CACHEDIR.TAG"] {
        let metadata = std::fs::metadata(root.join(name)).unwrap();
        assert_eq!(metadata.mode() & 0o777, 0o600);
        assert_eq!(metadata.uid(), nix::unistd::geteuid().as_raw());
    }
    assert_eq!(std::fs::metadata(root).unwrap().mode() & 0o777, 0o700);
}

fn temporary_files(parent: &Path) -> usize {
    std::fs::read_dir(parent.join(ROOT))
        .unwrap_or_else(|_| panic!("private root"))
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
        .count()
}

#[test]
fn process_starting_during_copy_refuses_and_does_not_block_source_writes() {
    let source = fixture(10);
    let scratch = scratch();
    let store = Store::open(scratch.path(), &control()).unwrap_or_else(|e| panic!("{e}"));
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
    assert_eq!(temporary_files(scratch.path()), 0);
}

#[test]
fn cancellation_removes_staging_and_process_check_failure_does_not_copy() {
    let source = fixture(10);
    let scratch = scratch();
    let store = Store::open(scratch.path(), &control()).unwrap_or_else(|e| panic!("{e}"));
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
    assert_eq!(temporary_files(scratch.path()), 0);
    let result = store.read_with(
        source.path(),
        &super::super::tests::control(),
        &mut || panic!("must not copy"),
        &mut || Err(ClosedError::ProcessCheck),
    );
    assert_eq!(result.err(), Some(ClosedError::ProcessCheck));
    assert_eq!(temporary_files(scratch.path()), 0);
}

#[cfg(unix)]
#[test]
fn source_symlinks_fifo_and_symlinked_private_roots_are_refused() {
    use std::os::unix::fs::symlink;
    let source = fixture(10);
    let scratch = scratch();
    let alias = scratch.path().join(ROOT);
    symlink(source.path(), &alias).unwrap_or_else(|_| panic!("symlink root"));
    assert_eq!(
        Store::open(scratch.path(), &control()).err(),
        Some(ClosedError::PrivateStorage)
    );
    let scratch2 = self::scratch();
    let store = Store::open(scratch2.path(), &control()).unwrap_or_else(|e| panic!("{e}"));
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
    let store = Store::open(scratch.path(), &control()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        store
            .read_with(source.path(), &control(), &mut || panic!("must not copy"), &mut || Ok(
                ()
            ))
            .err(),
        Some(ClosedError::TooLarge)
    );
    assert_eq!(temporary_files(scratch.path()), 0);
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
    let cache = scratch();
    let store = Store::open(cache.path(), &control()).unwrap();
    let mut copies = 0;
    for _ in 0..2 {
        let snapshot = store
            .read_with(
                source.path(),
                &control(),
                &mut || {
                    copies += 1;
                    Ok(())
                },
                &mut || Ok(()),
            )
            .unwrap();
        assert_eq!(
            snapshot.libraries[0].items[0]
                .data
                .fields
                .get("title")
                .map(String::as_str),
            Some("Committed Synthetic WAL Title")
        );
    }
    assert_eq!(copies, 1);
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

#[cfg(unix)]
#[test]
fn source_replacement_and_restored_mtime_changes_are_detected() {
    for replace in [true, false] {
        let source = fixture(10);
        let scratch = scratch();
        let store = Store::open(scratch.path(), &control()).unwrap_or_else(|e| panic!("{e}"));
        let mut attempts = 0;
        let result = store.read_with(
            source.path(),
            &control(),
            &mut || {
                attempts += 1;
                let path = source.path().join("zotero.sqlite");
                let old_time = std::fs::metadata(&path)
                    .unwrap_or_else(|_| panic!("synthetic metadata"))
                    .modified()
                    .unwrap_or_else(|_| panic!("synthetic time"));
                if replace {
                    let bytes = std::fs::read(&path).unwrap_or_default();
                    let replacement = source.path().join("replacement");
                    std::fs::write(&replacement, bytes).unwrap_or_else(|_| panic!("synthetic replacement"));
                    std::fs::rename(replacement, &path).unwrap_or_else(|_| panic!("synthetic rename"));
                } else {
                    edit(
                        source.path(),
                        &format!("UPDATE items SET version={} WHERE itemID=10", attempts + 20),
                    );
                }
                let file = OpenOptions::new()
                    .write(true)
                    .open(path)
                    .unwrap_or_else(|_| panic!("synthetic file"));
                file.set_modified(old_time)
                    .unwrap_or_else(|_| panic!("restore synthetic mtime"));
                Ok(())
            },
            &mut || Ok(()),
        );
        assert_eq!(result.err(), Some(ClosedError::Changing));
        assert_eq!(attempts, 3);
    }
}

#[cfg(unix)]
#[test]
fn native_process_guard_refuses_a_synthetic_zotero_named_process() {
    let child = platform::tests::NamedProcess::start("zotero");
    let _ = child.snapshot();
    let observed = platform::closed(&control());
    drop(child);
    assert_eq!(observed, Err(ClosedError::Running));
}
