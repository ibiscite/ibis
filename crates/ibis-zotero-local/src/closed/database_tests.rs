use super::*;
use crate::closed::tests::{edit, fixture, read_fixture};

#[test]
fn multipart_dates_follow_typed_text_without_losing_timestamps() {
    for (source, wanted) in [
        ("2020-00-00 Spring 2020", "Spring 2020"),
        ("0000-00-00 circa α", "circa α"),
        ("2020-01-01 ", ""),
        ("2020-99-01 typed", "2020-99-01 typed"),
        ("2020-01-99 typed", "2020-01-99 typed"),
        ("-020-01-01 typed", "-020-01-01 typed"),
        ("2020-02-03 04:05:06", "2020-02-03 04:05:06"),
        ("2020-02-03 04:05", "2020-02-03 04:05"),
        ("2020-00-00 2000-01-01 typed", "2000-01-01 typed"),
    ] {
        assert_eq!(multipart(source), wanted);
    }
    assert!(!date_field("book", "accessDate"));
    assert!(date_field("case", "dateDecided"));
    assert!(!date_field("book", "dateDecided"));
}

#[test]
fn rejects_views_virtual_tables_generated_columns_and_duplicate_identity() {
    for sql in [
        "ALTER TABLE fieldsCombined RENAME TO oldfields; CREATE VIEW fieldsCombined AS SELECT * FROM oldfields;",
        "DROP TABLE fieldsCombined; CREATE VIRTUAL TABLE fieldsCombined USING fts5(fieldID,fieldName);",
        "ALTER TABLE fieldsCombined ADD COLUMN generated TEXT GENERATED ALWAYS AS (fieldName);",
        "UPDATE items SET key='SYNTH001' WHERE itemID=12; DELETE FROM deletedItems;",
        "INSERT INTO itemData VALUES(10,1,1);",
        "UPDATE creators SET fieldMode=2;",
        "INSERT INTO itemCreators VALUES(10,1,1,0);",
    ] {
        let dir = fixture(10);
        edit(dir.path(), sql);
        assert!(read_fixture(dir.path()).is_err());
    }
}

#[test]
fn null_trash_rows_do_not_hide_the_entire_library() {
    let dir = fixture(10);
    edit(dir.path(), "INSERT INTO deletedItems VALUES(NULL);");
    assert_eq!(read_fixture(dir.path()).map(|s| s.libraries[0].items.len()), Ok(1));
}

#[test]
fn oversized_fields_and_excessive_creators_are_refused() {
    let dir = fixture(10);
    edit(
        dir.path(),
        "UPDATE itemDataValues SET value=printf('%.*c',300000,'x') WHERE valueID=1;",
    );
    assert_eq!(read_fixture(dir.path()).err(), Some(ClosedError::TooLarge));
    let dir = fixture(10);
    edit(
        dir.path(),
        "WITH RECURSIVE n(x) AS (SELECT 2 UNION ALL SELECT x+1 FROM n WHERE x<4096) INSERT INTO itemCreators SELECT 10,1,1,x FROM n;",
    );
    assert_eq!(read_fixture(dir.path()).err(), Some(ClosedError::TooLarge));
}

#[test]
fn journal_recovery_is_refused_without_interpreting_external_paths() {
    let dir = fixture(10);
    let outside = tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic outside"));
    let sentinel = outside.path().join("sentinel");
    std::fs::write(&sentinel, b"synthetic sentinel").unwrap_or_else(|_| panic!("sentinel"));
    let mut journal = vec![0u8; 512];
    journal.extend_from_slice(sentinel.as_os_str().as_encoded_bytes());
    std::fs::write(dir.path().join("zotero.sqlite-journal"), journal).unwrap_or_else(|_| panic!("synthetic journal"));
    assert_eq!(read_fixture(dir.path()).err(), Some(ClosedError::NeedsRecovery));
    assert_eq!(std::fs::read(sentinel).unwrap_or_default(), b"synthetic sentinel");
}

#[test]
fn dangling_bibliographic_rows_are_refused_instead_of_silently_dropped() {
    for sql in [
        "UPDATE items SET itemTypeID=999 WHERE itemID=10;",
        "UPDATE itemData SET fieldID=999 WHERE itemID=10 AND fieldID=1;",
        "UPDATE itemData SET valueID=999 WHERE itemID=10 AND fieldID=1;",
        "UPDATE itemCreators SET creatorID=999 WHERE itemID=10 AND orderIndex=0;",
        "UPDATE itemCreators SET creatorTypeID=999 WHERE itemID=10 AND orderIndex=1;",
    ] {
        let dir = fixture(10);
        edit(dir.path(), sql);
        assert_eq!(read_fixture(dir.path()).err(), Some(ClosedError::Damaged));
    }
}
