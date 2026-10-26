use super::*;

#[test]
fn attachment_text_obeys_the_same_value_and_cumulative_budget() {
    for content_type in [false, true] {
        let (data, scratch) = temp();
        let mut f = Fixture::new(&data.path().join("zotero.sqlite"), 125);
        let huge = "x".repeat(2 << 20);
        f.item(1, 1, 1, "PARENT01", "2024-01-01 00:00:00", &[(1, "Synthetic")]);
        f.item(
            2,
            1,
            4,
            "ATTACH01",
            "2024-01-01 00:00:00",
            &[(5, if content_type { "https://example.org/" } else { &huge })],
        );
        f.db.execute(
            "INSERT INTO itemAttachments VALUES (2, 1, 0, ?1)",
            [if content_type { &huge } else { "application/pdf" }],
        )
        .unwrap_or_else(|e| panic!("{e}"));
        drop(f);
        assert_eq!(
            read(data.path(), scratch.path(), &[Library::User], None),
            Err(SnapshotError::TooLarge)
        );
    }
}

#[test]
fn selected_shadow_keys_fail_closed_but_unrelated_rows_do_not() {
    let (data, scratch) = temp();
    let mut f = Fixture::new(&data.path().join("zotero.sqlite"), 125);
    f.item(1, 1, 1, "SHADOW01", "2024-01-01 00:00:00", &[(1, "Synthetic")]);
    drop(f);
    std::fs::create_dir(data.path().join("better-bibtex")).unwrap_or_else(|e| panic!("{e}"));
    let cache = data.path().join("better-bibtex/read-only.json");
    for (key, library, refused) in [("SHADOW01", 1, true), ("ANOTHER1", 1, false), ("SHADOW01", 2, false)] {
        let rows =
            serde_json::json!([{ "itemID": 1, "itemKey": key, "libraryID": library, "citationKey": "synthetic-key" }]);
        std::fs::write(&cache, rows.to_string()).unwrap_or_else(|e| panic!("{e}"));
        let read = read(data.path(), scratch.path(), &[Library::User], None);
        if refused {
            assert_eq!(read, Err(SnapshotError::UnsupportedKeys));
        } else {
            assert_eq!(loaded(read).0.references.len(), 1);
        }
    }
}

#[test]
fn duplicated_schema_mappings_share_the_cumulative_budget() {
    let (data, scratch) = temp();
    let f = Fixture::new(&data.path().join("zotero.sqlite"), 125);
    f.db.execute(
        "UPDATE fieldsCombined SET fieldName = ?1 WHERE fieldID = 1",
        ["x".repeat(128 << 10)],
    )
    .unwrap_or_else(|e| panic!("{e}"));
    f.sql(
        "WITH RECURSIVE rows(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM rows WHERE n < 3000)
           INSERT INTO baseFieldMappingsCombined SELECT 1, 1, 1 FROM rows;",
    );
    drop(f);
    assert_eq!(
        read(data.path(), scratch.path(), &[Library::User], None),
        Err(SnapshotError::TooLarge)
    );
}

#[test]
fn a_superseded_read_stops_during_copy_and_removes_its_private_copy() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let (data, scratch) = temp();
    let mut f = Fixture::new(&data.path().join("zotero.sqlite"), 125);
    f.item(1, 1, 1, "CANCEL01", "2024-01-01 00:00:00", &[(1, &"x".repeat(3 << 20))]);
    drop(f);
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    assert_eq!(
        read_cancellable(
            data.path(),
            scratch.path(),
            &[Library::User],
            None,
            Arc::new(move || counted.fetch_add(1, Ordering::SeqCst) >= 6)
        ),
        Err(SnapshotError::Cancelled)
    );
    assert!(calls.load(Ordering::SeqCst) >= 7);
    assert_eq!(std::fs::read_dir(scratch.path()).map(Iterator::count).ok(), Some(0));
}

#[test]
fn coordination_excludes_sqlite_writers_even_when_wal_sidecars_are_absent() {
    for wal in [false, true] {
        let (data, scratch) = temp();
        let mut f = Fixture::new(&data.path().join("zotero.sqlite"), 125);
        f.item(1, 1, 1, "LOCKED01", "2024-01-01 00:00:00", &[(1, "Before")]);
        if wal {
            f.sql("PRAGMA journal_mode = WAL;");
        }
        drop(f);
        assert!(!data.path().join("zotero.sqlite-shm").exists());
        let before = contents(data.path());
        let seen = fingerprint(data.path()).unwrap_or_else(|e| panic!("{e}"));
        let control = Control {
            deadline: Instant::now() + Duration::from_secs(10),
            cancelled: Arc::new(|| false),
        };
        let mut sources = locks::Sources::acquire(data.path(), &seen, &control).unwrap_or_else(|e| panic!("{e}"));
        let probe = |expect: &str| {
            std::process::Command::new(std::env::current_exe().unwrap_or_else(|e| panic!("{e}")))
                .args(["--exact", "snapshot::tests::hostile::sqlite_writer_child", "--ignored"])
                .env("IBIS_SNAPSHOT_CHILD_DATA", data.path())
                .env("IBIS_SNAPSHOT_CHILD_EXPECT", expect)
                .output()
                .unwrap_or_else(|e| panic!("{e}"))
        };
        #[cfg(windows)]
        assert!(
            std::fs::OpenOptions::new()
                .write(true)
                .open(data.path().join("zotero.sqlite"))
                .is_err()
        );
        let blocked = probe("blocked");
        assert!(blocked.status.success(), "{}", String::from_utf8_lossy(&blocked.stdout));
        sources
            .copy_to(scratch.path(), &control)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            contents(data.path()),
            before,
            "no source file was created, changed or removed"
        );
        assert_eq!(
            fingerprint(data.path()),
            Ok(seen),
            "source sizes and mtimes are unchanged"
        );
        drop(sources);
        #[cfg(windows)]
        assert!(
            std::fs::OpenOptions::new()
                .write(true)
                .open(data.path().join("zotero.sqlite"))
                .is_ok()
        );
        let written = probe("write");
        assert!(written.status.success(), "{}", String::from_utf8_lossy(&written.stdout));
        let (result, _) = loaded(read(data.path(), scratch.path(), &[Library::User], None));
        assert_eq!(result.references[0].title.as_deref(), Some("After!"));
    }
}

#[test]
#[ignore = "subprocess used by coordination_excludes_sqlite_writers"]
fn sqlite_writer_child() {
    let data = std::env::var_os("IBIS_SNAPSHOT_CHILD_DATA").unwrap_or_else(|| panic!("synthetic child data"));
    let blocked = std::env::var("IBIS_SNAPSHOT_CHILD_EXPECT").as_deref() == Ok("blocked");
    let db = match Connection::open(PathBuf::from(data).join("zotero.sqlite")) {
        Ok(db) => db,
        Err(error) if cfg!(windows) && blocked => {
            assert_eq!(error.sqlite_error_code(), Some(rusqlite::ErrorCode::CannotOpen));
            return;
        }
        Err(error) => panic!("{error}"),
    };
    db.busy_timeout(Duration::ZERO).unwrap_or_else(|e| panic!("{e}"));
    let outcome = db.execute_batch("BEGIN IMMEDIATE; UPDATE itemDataValues SET value = 'After!' WHERE value = 'Before'; COMMIT; PRAGMA wal_checkpoint(TRUNCATE);");
    if blocked {
        assert!(matches!(
            outcome.err().and_then(|e| e.sqlite_error_code()),
            Some(
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked | rusqlite::ErrorCode::ReadOnly
            )
        ));
    } else {
        outcome.unwrap_or_else(|e| panic!("{e}"));
    }
}

#[test]
fn rollback_super_journal_paths_are_refused_before_sqlite_opens_the_copy() {
    let (data, scratch) = temp();
    let f = Fixture::new(&data.path().join("zotero.sqlite"), 125);
    drop(f);
    // SQLite's super-journal tail stores a name, its length/checksum, and
    // journal magic. No external path is ever probed by this test.
    let name = br"\\synthetic-server\synthetic-share\never-probed";
    let mut journal = vec![0u8; 512];
    journal.extend_from_slice(name);
    journal.extend_from_slice(&(name.len() as u32).to_be_bytes());
    journal.extend_from_slice(&name.iter().map(|b| u32::from(*b)).sum::<u32>().to_be_bytes());
    journal.extend_from_slice(&[0xd9, 0xd5, 0x05, 0xf9, 0x20, 0xa1, 0x63, 0xd7]);
    std::fs::write(data.path().join("zotero.sqlite-journal"), journal).unwrap_or_else(|e| panic!("{e}"));
    let before = contents(data.path());
    assert_eq!(
        read(data.path(), scratch.path(), &[Library::User], None),
        Err(SnapshotError::NeedsRecovery)
    );
    assert_eq!(contents(data.path()), before);
    assert_eq!(std::fs::read_dir(scratch.path()).map(Iterator::count).ok(), Some(0));
}
