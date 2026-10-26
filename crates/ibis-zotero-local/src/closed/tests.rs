use super::*;
use std::path::PathBuf;

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
    Control {
        deadline: Instant::now() + Duration::from_secs(30),
        cancelled: Arc::new(|| false),
    }
}

pub(super) fn read_fixture(dir: &Path) -> Result<ClosedSnapshot, ClosedError> {
    let scratch = tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic scratch"));
    let store = private_copy::Store::open(scratch.path())?;
    store.read_with(dir, &control(), &mut || Ok(()), &mut || Ok(()))
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
                _ => 130,
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
        assert_eq!(snapshot.libraries[1].items.len(), 1);
        assert_eq!(
            std::fs::read(dir.path().join("zotero.sqlite")).unwrap_or_default(),
            original
        );
        assert!(!dir.path().join("zotero.sqlite-shm").exists());
    }
}

#[test]
fn rejects_old_new_damaged_and_missing_databases() {
    for (version, error) in [(122, ClosedError::TooOld), (131, ClosedError::TooNew)] {
        let dir = fixture(10);
        edit(dir.path(), &format!("UPDATE version SET version={version}"));
        assert_eq!(read_fixture(dir.path()).err(), Some(error));
    }
    let dir = tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic dir"));
    assert_eq!(read_fixture(dir.path()).err(), Some(ClosedError::NotFound));
    std::fs::write(dir.path().join("zotero.sqlite"), b"synthetic damaged database")
        .unwrap_or_else(|_| panic!("synthetic bytes"));
    assert_eq!(read_fixture(dir.path()).err(), Some(ClosedError::Damaged));
}

#[test]
fn reports_cloud_sync_without_disclosing_content_in_debug() {
    assert!(cloud_synced(&PathBuf::from(
        "/synthetic/Library/CloudStorage/Example/Zotero"
    )));
    assert!(cloud_synced(Path::new("/synthetic/OneDrive - Example/Zotero")));
    assert!(!cloud_synced(Path::new("/synthetic/plain/Zotero")));
    let dir = fixture(10);
    let snapshot = read_fixture(dir.path()).unwrap_or_else(|e| panic!("{e}"));
    assert!(!format!("{snapshot:?}").contains("Synthetic"));
    for e in [
        ClosedError::Damaged,
        ClosedError::Unreadable,
        ClosedError::PrivateStorage,
    ] {
        assert!(!e.to_string().contains("synthetic"));
    }
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
