use super::*;

pub(super) fn fixture(version: u8) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic database directory"));
    let db =
        rusqlite::Connection::open(dir.path().join("zotero.sqlite")).unwrap_or_else(|_| panic!("synthetic database"));
    db.execute_batch(include_str!("../../fixtures/closed/schema.sql"))
        .unwrap_or_else(|_| panic!("synthetic schema"));
    let sql = match version {
        7 => include_str!("../../fixtures/closed/zotero7.sql"),
        8 => include_str!("../../fixtures/closed/zotero8.sql"),
        9 => include_str!("../../fixtures/closed/zotero9.sql"),
        10 => include_str!("../../fixtures/closed/zotero10.sql"),
        _ => panic!("synthetic version"),
    };
    db.execute_batch(sql)
        .unwrap_or_else(|_| panic!("synthetic version schema"));
    drop(db);
    dir
}

pub(super) fn edit(dir: &Path, sql: &str) {
    let db = rusqlite::Connection::open(dir.join("zotero.sqlite")).unwrap_or_else(|_| panic!("synthetic edit"));
    db.execute_batch(sql).unwrap_or_else(|_| panic!("synthetic SQL"));
}

pub(super) fn control() -> Control {
    Control::new(Arc::new(|| false))
}

pub(super) fn read_fixture(dir: &Path) -> Result<ClosedSnapshot, ClosedError> {
    let scratch = tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic scratch"));
    let control = control();
    let store = private_copy::Store::open(scratch.path(), &control)?;
    store.read_with(dir, &control, &mut || Ok(()), &mut || Ok(()))
}

#[test]
fn closed_reader_copies_and_reads_synthetic_database() {
    let source = fixture(7);
    assert_eq!(read_fixture(source.path()).map(|s| s.schema_version), Ok(123));
}

#[test]
fn all_four_versions_return_neutral_fields_groups_and_ordered_creators() {
    for version in 7..=10 {
        let dir = fixture(version);
        let original = std::fs::read(dir.path().join("zotero.sqlite")).unwrap_or_default();
        let snapshot = read_fixture(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            snapshot.schema_version,
            match version {
                7 | 8 => 123,
                9 => 125,
                _ => 131,
            }
        );
        assert_eq!(snapshot.libraries.len(), 2);
        let personal = &snapshot.libraries[0];
        assert_eq!(personal.items.len(), 1);
        let item = &personal.items[0];
        assert_eq!(item.data.key, "SYNTH001");
        assert_eq!(item.data.item_type, "book");
        assert_eq!(item.data.fields.get("date").map(String::as_str), Some("Spring 2020"));
        assert_eq!(
            item.data.fields.get("accessDate").map(String::as_str),
            Some("2020-02-03 04:05:06")
        );
        assert_eq!(
            item.metadata.stored_fields.get("date").map(String::as_str),
            Some("2020-00-00 Spring 2020")
        );
        assert_eq!(item.data.creators.len(), 2);
        assert_eq!(item.data.creators[0].creator_type, "author");
        assert_eq!(item.data.creators[0].last_name, "Example");
        assert_eq!(item.data.creators[1].name, "Synthetic Institute");
        assert_eq!(item.data.creators[1].field_mode, 1);
        assert_eq!(snapshot.libraries[1].group_id, Some(4321));
        assert_eq!(snapshot.libraries[1].name, "Invented Reading Group");
        assert_eq!(snapshot.libraries[1].items.len(), 1);
        assert_eq!(
            std::fs::read(dir.path().join("zotero.sqlite")).unwrap_or_default(),
            original
        );
        assert!(!dir.path().join("zotero.sqlite-shm").exists());
    }
}

#[test]
fn rejects_old_damaged_and_missing_databases() {
    let old = fixture(10);
    edit(old.path(), "UPDATE version SET version=122");
    assert_eq!(read_fixture(old.path()).err(), Some(ClosedError::TooOld));
    let dir = tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic dir"));
    assert_eq!(read_fixture(dir.path()).err(), Some(ClosedError::NotFound));
    std::fs::write(dir.path().join("zotero.sqlite"), b"synthetic damaged database")
        .unwrap_or_else(|_| panic!("synthetic bytes"));
    assert_eq!(read_fixture(dir.path()).err(), Some(ClosedError::Damaged));
}

#[test]
fn cloud_synced_synthetic_directory_returns_a_fixed_warning() {
    let source = fixture(10);
    let root = tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic cloud root"));
    let data = root.path().join("Dropbox").join("Zotero");
    std::fs::create_dir_all(&data).unwrap_or_else(|_| panic!("synthetic cloud dir"));
    std::fs::copy(source.path().join("zotero.sqlite"), data.join("zotero.sqlite"))
        .unwrap_or_else(|_| panic!("synthetic cloud db"));
    assert_eq!(
        read_fixture(&data).map(|s| s.warnings),
        Ok(vec![ClosedWarning::CloudSyncedDirectory])
    );
}

#[test]
fn newer_compatible_schema_reads_with_one_source_warning_and_leaves_source_untouched() {
    let dir = fixture(10);
    edit(dir.path(), "UPDATE version SET version=132");
    let before = std::fs::read(dir.path().join("zotero.sqlite")).unwrap_or_else(|_| panic!("synthetic source bytes"));
    let snapshot = read_fixture(dir.path()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        snapshot.libraries[0].items[0].data.fields["title"],
        "The Synthetic Atlas"
    );
    assert_eq!(snapshot.warnings, vec![ClosedWarning::NewerThanTested]);
    assert_eq!(
        std::fs::read(dir.path().join("zotero.sqlite")).unwrap_or_else(|_| panic!("synthetic source bytes")),
        before
    );
}

#[test]
fn newer_schema_refuses_missing_columns_and_changed_lookup_shapes() {
    for (change, table) in [
        ("ALTER TABLE items DROP COLUMN dateModified", ZoteroTable::Items),
        (
            "ALTER TABLE fieldsCombined DROP COLUMN fieldName",
            ZoteroTable::FieldsCombined,
        ),
        ("DROP TABLE itemTypesCombined", ZoteroTable::ItemTypesCombined),
        (
            "DROP TABLE itemDataValues; CREATE TABLE itemDataValues(valueID INTEGER PRIMARY KEY, value TEXT UNIQUE)",
            ZoteroTable::ItemDataValues,
        ),
        (
            "DROP TABLE creatorTypes; CREATE TABLE creatorTypes(creatorTypeID INTEGER PRIMARY KEY, creatorType BLOB)",
            ZoteroTable::CreatorTypes,
        ),
        (
            "DROP TABLE fieldsCombined; CREATE TABLE fieldsCombined(fieldID INT, fieldName TEXT)",
            ZoteroTable::FieldsCombined,
        ),
        (
            "ALTER TABLE items ADD COLUMN computed TEXT GENERATED ALWAYS AS (key) VIRTUAL",
            ZoteroTable::Items,
        ),
    ] {
        let dir = fixture(10);
        edit(dir.path(), "UPDATE version SET version=132");
        edit(dir.path(), change);
        let error = read_fixture(dir.path())
            .err()
            .unwrap_or_else(|| panic!("incompatible schema accepted"));
        assert_eq!(error, ClosedError::TooNew { table });
        assert_eq!(
            error.to_string(),
            "this Zotero database is newer than Ibis knows; update Ibis or open Zotero",
            "{change}"
        );
    }
}

#[test]
fn the_text_cache_is_read_from_a_regular_file_in_the_attachments_folder() {
    let data = tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic data directory"));
    let folder = data.path().join("storage").join("FILE0001");
    std::fs::create_dir_all(&folder).unwrap_or_else(|_| panic!("synthetic storage"));
    assert_eq!(read_text_cache(data.path(), "FILE0001", Arc::new(|| false)), Ok(None));
    std::fs::write(
        folder.join(".zotero-ft-cache"),
        "Invented page one.\u{c}Invented page two.",
    )
    .unwrap_or_else(|_| panic!("synthetic cache"));
    assert_eq!(
        read_text_cache(data.path(), "FILE0001", Arc::new(|| false)),
        Ok(Some("Invented page one.\u{c}Invented page two.".into()))
    );
    assert_eq!(read_text_cache(data.path(), "NOFOLDER", Arc::new(|| false)), Ok(None));
    let large = data.path().join("storage").join("LARGE001");
    std::fs::create_dir_all(&large).unwrap_or_else(|_| panic!("synthetic storage"));
    std::fs::write(
        large.join(".zotero-ft-cache"),
        vec![b'a'; crate::fulltext::MAX_TEXT_BYTES + 1],
    )
    .unwrap_or_else(|_| panic!("synthetic cache"));
    assert_eq!(
        read_text_cache(data.path(), "LARGE001", Arc::new(|| false)),
        Err(ClosedError::TooLarge)
    );
    assert_eq!(
        read_text_cache(data.path(), "../storage", Arc::new(|| false)),
        Err(ClosedError::UnsafeSource)
    );
    #[cfg(unix)]
    {
        let linked = data.path().join("storage").join("LINKED01");
        std::fs::create_dir_all(&linked).unwrap_or_else(|_| panic!("synthetic storage"));
        std::os::unix::fs::symlink(folder.join(".zotero-ft-cache"), linked.join(".zotero-ft-cache"))
            .unwrap_or_else(|_| panic!("synthetic link"));
        assert!(
            read_text_cache(data.path(), "LINKED01", Arc::new(|| false)).is_err(),
            "a linked cache is not followed"
        );
    }
}

#[cfg(unix)]
#[test]
fn text_cache_refuses_linked_storage_directories() {
    let data = tempfile::tempdir().unwrap();
    let storage = data.path().join("storage");
    let secret = storage.join("SECRET01");
    std::fs::create_dir_all(&secret).unwrap();
    std::fs::write(secret.join(".zotero-ft-cache"), "Invented secret text").unwrap();
    std::os::unix::fs::symlink("SECRET01", storage.join("ALLOW001")).unwrap();
    assert!(
        read_text_cache(data.path(), "ALLOW001", Arc::new(|| false)).is_err(),
        "a sibling attachment is not followed"
    );

    let other = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(&storage, other.path().join("storage")).unwrap();
    assert!(
        read_text_cache(other.path(), "SECRET01", Arc::new(|| false)).is_err(),
        "storage itself is not followed"
    );
    let linked_data = other.path().join("linked-data");
    std::os::unix::fs::symlink(data.path(), &linked_data).unwrap();
    assert_eq!(
        read_text_cache(&linked_data, "SECRET01", Arc::new(|| false)),
        Ok(Some("Invented secret text".into())),
        "links in the selected root are resolved once"
    );
    assert!(
        read_text_cache(&linked_data, "ALLOW001", Arc::new(|| false)).is_err(),
        "resolving the root does not authorize links below it"
    );
}

fn stopped_cache_read(cancel: bool) {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    let token = cancelled.clone();
    let control = Control::within(
        Arc::new(move || token.load(Ordering::SeqCst)),
        if cancel {
            Duration::from_secs(5)
        } else {
            Duration::from_millis(50)
        },
    );
    let (started, ready) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let (finished, result) = mpsc::channel();
    let caller = std::thread::spawn(move || {
        // A read that stalls in the kernel until released.
        let answer = control.io(move |_| {
            started.send(()).unwrap();
            released.recv().unwrap();
            Ok(())
        });
        finished.send(answer).unwrap();
    });
    ready.recv_timeout(Duration::from_secs(1)).unwrap();
    if cancel {
        cancelled.store(true, Ordering::SeqCst);
    }
    // The caller must be released while the injected read is still stalled.
    let answer = result.recv_timeout(Duration::from_millis(500));
    release.send(()).unwrap();
    caller.join().unwrap();
    assert_eq!(
        answer.ok(),
        Some(Err(if cancel {
            ClosedError::Cancelled
        } else {
            ClosedError::TimedOut
        }))
    );
}

#[test]
fn text_cache_deadline_releases_the_caller_during_a_stalled_read() {
    stopped_cache_read(false);
}
