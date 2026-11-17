use super::*;
use crate::closed::tests::{edit, fixture, read_fixture};

#[test]
fn break_foundation_orphaned_library_is_not_silently_discarded() {
    let dir = fixture(10);
    edit(dir.path(), "UPDATE items SET libraryID=999 WHERE itemID=10;");
    let result = read_fixture(dir.path());
    assert!(
        result.is_err(),
        "orphaned bibliographic item disappeared in a successful snapshot: {:?}",
        result.map(|snapshot| (
            snapshot
                .libraries
                .iter()
                .map(|library| library.items.len())
                .sum::<usize>(),
            snapshot.warnings
        ))
    );
}

#[test]
fn rejects_views_virtual_tables_generated_columns_and_duplicate_identity() {
    for sql in [
        "ALTER TABLE fieldsCombined RENAME TO oldfields; CREATE VIEW fieldsCombined AS SELECT * FROM oldfields;",
        "DROP TABLE fieldsCombined; CREATE VIRTUAL TABLE fieldsCombined USING fts5(fieldID,fieldName);",
        "ALTER TABLE fieldsCombined ADD COLUMN generated TEXT GENERATED ALWAYS AS (fieldName);",
        "UPDATE items SET key='SYNTH001' WHERE itemID=12; DELETE FROM deletedItems;",
        "UPDATE items SET key='SYNTH001' WHERE itemID=13;",
        "UPDATE items SET key='SYNTH001' WHERE itemID=12;",
        "UPDATE libraries SET type='invented' WHERE libraryID=1;",
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
fn oversized_fields_and_excessive_creators_leave_other_items_readable() {
    for problem in ["field", "creators", "field-count", "creator-name"] {
        let dir = fixture(10);
        edit(
            dir.path(),
            "INSERT INTO items VALUES(17,1,'NORMAL01',1,1,'2020-01-01 00:00:00','2020-01-01 00:00:00');
         INSERT INTO itemDataValues VALUES(999,'An unaffected synthetic book');
         INSERT INTO itemData VALUES(17,1,999);",
        );
        let sql = match problem {
            "creators" => {
                "WITH RECURSIVE n(x) AS (SELECT 2 UNION ALL SELECT x+1 FROM n WHERE x<5153)
         INSERT INTO creators SELECT x+100,'Synthetic','Collaborator '||x,0 FROM n;
         WITH RECURSIVE n(x) AS (SELECT 2 UNION ALL SELECT x+1 FROM n WHERE x<5153)
         INSERT INTO itemCreators SELECT 10,x+100,1,x FROM n;"
            }
            "field-count" => {
                "WITH RECURSIVE n(x) AS (SELECT 6 UNION ALL SELECT x+1 FROM n WHERE x<262)
                INSERT INTO fieldsCombined SELECT x,'syntheticField'||x FROM n;
                INSERT INTO itemData SELECT 10,fieldID,999 FROM fieldsCombined WHERE fieldID>=6;"
            }
            "creator-name" => "UPDATE creators SET firstName=printf('%.*c',300000,'x') WHERE creatorID=1;",
            _ => "UPDATE itemDataValues SET value=printf('%.*c',300000,'x') WHERE valueID=1;",
        };
        edit(dir.path(), sql);
        let snapshot = read_fixture(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(snapshot.libraries[0].items.len(), 1);
        assert_eq!(snapshot.libraries[0].items[0].data.key, "NORMAL01");
        assert_eq!(
            snapshot.libraries[0].items[0]
                .data
                .fields
                .get("title")
                .map(String::as_str),
            Some("An unaffected synthetic book")
        );
        assert!(
            snapshot.warnings.contains(&super::ClosedWarning::ItemTooLarge {
                library_id: 1,
                key: "SYNTH001".into(),
            }),
            "omitted item identity must be reported"
        );
        assert_eq!(snapshot.warnings.len(), if problem == "field" { 2 } else { 1 });
        assert!(!format!("{:?}", snapshot.warnings).contains("SYNTH001"));
    }
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

#[test]
fn attachments_carry_zoteros_index_counts_and_never_fail_a_read() {
    let dir = fixture(10);
    edit(
        dir.path(),
        "INSERT INTO itemAttachments VALUES(14,10,0,'application/pdf','storage:invented.pdf');
         INSERT INTO items VALUES(17,1,'LINK0001',1,4,'2020-01-01 00:00:00','2020-01-01 00:00:00');
         INSERT INTO itemAttachments VALUES(17,10,3,NULL,NULL);
         INSERT INTO fulltextItems(itemID,indexedPages,totalPages) VALUES(14,2,12);
         INSERT INTO itemDataValues VALUES(900,'Invented full text');
         INSERT INTO itemData VALUES(14,1,900);",
    );
    let snapshot = read_fixture(dir.path()).unwrap();
    let attachments = snapshot.attachments.unwrap();
    assert_eq!(attachments.len(), 2);
    let pdf = attachments
        .iter()
        .find(|attachment| attachment.key == "FILE0001")
        .unwrap();
    assert_eq!((pdf.library_id, pdf.parent.as_str()), (1, "SYNTH001"));
    assert_eq!(
        (pdf.title.as_str(), pdf.content_type.as_str()),
        ("Invented full text", "application/pdf")
    );
    assert!(!pdf.link_only);
    assert_eq!(pdf.path, "storage:invented.pdf");
    assert!(
        pdf.indexed
            == Some(IndexedCounts {
                indexed_pages: Some(2),
                total_pages: Some(12)
            })
    );
    let link = attachments
        .iter()
        .find(|attachment| attachment.key == "LINK0001")
        .unwrap();
    assert!(link.link_only && link.indexed.is_none() && link.content_type.is_empty() && link.path.is_empty());

    // A layout without Zotero's full-text table still reads every reference.
    edit(dir.path(), "DROP TABLE fulltextItems;");
    let snapshot = read_fixture(dir.path()).unwrap();
    assert!(snapshot.attachments.is_none());
    assert_eq!(snapshot.libraries[0].items.len(), 1);
}
