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
    journal[0] = 0xd9;
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

#[test]
fn unindexed_bibliographic_queries_stream_before_the_allocation_budget() {
    // The file is untrusted: table/column names cannot imply that Zotero's
    // usual indexes exist. SQLite must not build large sorts or automatic
    // indexes before the Rust row/text budget receives its first row.
    let mut db = Connection::open_in_memory().unwrap_or_else(|_| panic!("synthetic memory db"));
    let schema = include_str!("../../fixtures/closed/schema.sql")
        .replace("PRIMARY KEY", "")
        .replace("UNIQUE", "");
    db.execute_batch(&schema)
        .unwrap_or_else(|_| panic!("unindexed synthetic schema"));
    configure(&mut db, &crate::closed::tests::control()).unwrap_or_else(|e| panic!("{e}"));
    for sql in READ_SQL {
        let mut statement = db
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .unwrap_or_else(|_| panic!("query plan"));
        let plan = statement
            .query_map([], |r| r.get::<_, String>(3))
            .and_then(Iterator::collect::<rusqlite::Result<Vec<_>>>)
            .unwrap_or_else(|_| panic!("synthetic plan"));
        assert!(
            !plan
                .iter()
                .any(|p| p.contains("TEMP B-TREE") || p.contains("AUTOMATIC")),
            "{plan:?}"
        );
    }
}

#[test]
fn excessive_schema_objects_are_refused_before_they_are_materialized() {
    let dir = fixture(10);
    let mut sql = String::new();
    for i in 0..1100 {
        sql.push_str(&format!("CREATE TABLE synthetic_extra_{i}(x);"));
    }
    edit(dir.path(), &sql);
    assert!(read_fixture(dir.path()).is_err());
}

#[test]
fn forged_wal_page_counts_are_refused_before_sqlite_allocates_a_bitmap() {
    let dir = fixture(10);
    let db = Connection::open(dir.path().join("zotero.sqlite")).unwrap_or_else(|_| panic!("synthetic WAL database"));
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE synthetic_new(x); INSERT INTO synthetic_new VALUES('invented');").unwrap_or_else(|_|panic!("synthetic WAL commit"));
    let main = std::fs::read(dir.path().join("zotero.sqlite")).unwrap_or_default();
    let mut wal = std::fs::read(dir.path().join("zotero.sqlite-wal")).unwrap_or_default();
    drop(db);
    let be = |b: &[u8]| u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
    let little = be(&wal[..4]) == 0x377f0682;
    let size = be(&wal[8..12]) as usize;
    let frames = (wal.len() - 32) / (24 + size);
    let claimed = 16_777_216u32; // old quick_check allocates only2MiB in this red test.
    let mut s0 = be(&wal[24..28]);
    let mut s1 = be(&wal[28..32]);
    for i in 0..frames {
        let start = 32 + i * (24 + size);
        if be(&wal[start..start + 4]) == 1 {
            wal[start + 24 + 28..start + 24 + 32].copy_from_slice(&claimed.to_be_bytes());
        }
        if i == frames - 1 {
            wal[start + 4..start + 8].copy_from_slice(&claimed.to_be_bytes());
        }
        for range in [start..start + 8, start + 24..start + 24 + size] {
            for pair in wal[range].as_chunks::<8>().0 {
                let word = |b: &[u8]| {
                    if little {
                        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
                    } else {
                        be(b)
                    }
                };
                s0 = s0.wrapping_add(word(&pair[..4])).wrapping_add(s1);
                s1 = s1.wrapping_add(word(&pair[4..])).wrapping_add(s0);
            }
        }
        wal[start + 16..start + 20].copy_from_slice(&s0.to_be_bytes());
        wal[start + 20..start + 24].copy_from_slice(&s1.to_be_bytes());
    }
    std::fs::write(dir.path().join("zotero.sqlite"), main).unwrap_or_else(|_| panic!("restore synthetic main"));
    std::fs::write(dir.path().join("zotero.sqlite-wal"), wal).unwrap_or_else(|_| panic!("restore synthetic WAL"));
    // No gigantic allocation is attempted: this claim is deliberately small
    // enough for the pre-fix regression, but exceeds the reader's logical cap.
    assert_eq!(read_fixture(dir.path()).err(), Some(ClosedError::TooLarge));
}

#[test]
fn numeric_fields_use_zoteros_javascript_number_strings() {
    for (sql, expected) in [
        ("3", "3"),
        ("1.25", "1.25"),
        ("-0.0", "0"),
        ("1e-7", "1e-7"),
        ("1e-6", "0.000001"),
        ("1e20", "100000000000000000000"),
        ("1e21", "1e+21"),
        ("9007199254740993", "9007199254740992"),
    ] {
        let dir = fixture(10);
        edit(
            dir.path(),
            &format!("UPDATE itemDataValues SET value={sql} WHERE valueID=1;"),
        );
        let snapshot = read_fixture(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            snapshot.libraries[0].items[0]
                .data
                .fields
                .get("title")
                .map(String::as_str),
            Some(expected)
        );
    }
}

#[test]
fn zero_header_journal_is_ignored_only_in_the_private_copy() {
    let dir = fixture(7);
    let journal = dir.path().join("zotero.sqlite-journal");
    let bytes = vec![0u8; 512];
    std::fs::write(&journal, &bytes).unwrap_or_else(|_| panic!("synthetic cold journal"));
    assert!(read_fixture(dir.path()).is_ok());
    assert_eq!(std::fs::read(journal).unwrap_or_default(), bytes);
}

#[test]
fn database_queries_filter_discarded_item_rows_before_the_work_budget() {
    let dir = fixture(10);
    edit(
        dir.path(),
        "INSERT INTO itemData SELECT itemID,1,1 FROM items WHERE itemID>=12; INSERT INTO itemCreators SELECT itemID,1,1,0 FROM items WHERE itemID>=12;",
    );
    let db = Connection::open(dir.path().join("zotero.sqlite")).unwrap_or_else(|_| panic!("synthetic selection query"));
    let data: i64 = db
        .query_row(&format!("SELECT COUNT(*) FROM ({})", READ_SQL[2]), [], |r| r.get(0))
        .unwrap_or_else(|_| panic!("selected fields"));
    let creators: i64 = db
        .query_row(&format!("SELECT COUNT(*) FROM ({})", READ_SQL[3]), [], |r| r.get(0))
        .unwrap_or_else(|_| panic!("selected creators"));
    assert_eq!(data, 6);
    assert_eq!(creators, 2);
}

#[test]
fn null_item_values_are_absent_and_exact_numbers_stay_private() {
    let dir = fixture(10);
    edit(
        dir.path(),
        "UPDATE itemDataValues SET value=NULL WHERE valueID=2; UPDATE itemDataValues SET value=9007199254740993 WHERE valueID=1;",
    );
    let snapshot = read_fixture(dir.path()).unwrap_or_else(|e| panic!("{e}"));
    let item = &snapshot.libraries[0].items[0];
    assert!(!item.data.fields.contains_key("date"));
    assert_eq!(item.metadata.stored_values.get("date"), Some(&StoredValue::Null));
    assert_eq!(
        item.metadata.stored_values.get("title"),
        Some(&StoredValue::Integer(9007199254740993))
    );
    assert!(!format!("{:?}", item.metadata).contains("9007199254740993"));
}

#[test]
fn fifty_thousand_realistic_articles_fit_the_closed_reader() {
    let dir = fixture(10);
    edit(
        dir.path(),
        r#"
        BEGIN;
        DELETE FROM items; DELETE FROM itemData; DELETE FROM itemDataValues;
        DELETE FROM itemCreators; DELETE FROM deletedItems;
        INSERT INTO fieldsCombined VALUES(6,'abstractNote'),(7,'publicationTitle'),(8,'volume'),(9,'issue'),(10,'pages'),(11,'DOI'),(12,'url');
        INSERT INTO itemDataValues VALUES
          (1,'Synthetic large-library article'),(2,'2020-00-00 2020'),(3,'2020-01-01 00:00:00'),
          (4,'SyntheticLargeKey'),(5,'Invented metadata for a bounded library test'),
          (6,printf('%.*c',1700,'a')),(7,'Journal of Invented Scale'),(8,'12'),(9,'3'),
          (10,'101-120'),(11,'10.0000/synthetic-scale'),(12,'https://example.invalid/synthetic');
        WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<50000)
          INSERT INTO items SELECT 100+x,1,printf('S%07d',x),1,2,'2020-01-01 00:00:00','2020-02-01 00:00:00' FROM n;
        INSERT INTO itemData SELECT itemID,fieldID,fieldID FROM items CROSS JOIN fieldsCombined;
        INSERT INTO creators VALUES(3,'Bea','Invented',0);
        INSERT INTO creatorTypes VALUES(3,'translator');
        INSERT INTO itemCreators SELECT itemID,1,1,0 FROM items;
        INSERT INTO itemCreators SELECT itemID,2,2,1 FROM items;
        INSERT INTO itemCreators SELECT itemID,3,3,2 FROM items;
        COMMIT;
    "#,
    );
    let snapshot = read_fixture(dir.path()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(snapshot.libraries[0].items.len(), 50000);
    let last = &snapshot.libraries[0].items[49999];
    assert_eq!(last.data.key, "S0050000");
    assert_eq!(last.data.fields.len(), 12);
    assert_eq!(last.data.fields.get("abstractNote").map(String::len), Some(1700));
    assert_eq!(last.data.creators.len(), 3);
    assert_eq!(
        last.metadata
            .stored_fields
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["date"]
    );
}
