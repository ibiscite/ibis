//! A synthetic Zotero database, built here in Zotero's layout for the
//! tables Ibis reads, stands in for a real one: no real library is used.

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{Connection, params};

use super::*;

const LAYOUT: &str = "
CREATE TABLE version (schema TEXT PRIMARY KEY, version INT NOT NULL);
CREATE TABLE settings (setting TEXT, key TEXT, value, PRIMARY KEY (setting, key));
CREATE TABLE libraries (libraryID INTEGER PRIMARY KEY, type TEXT NOT NULL, editable INT NOT NULL DEFAULT 1);
CREATE TABLE groups (groupID INTEGER PRIMARY KEY, libraryID INT NOT NULL UNIQUE, name TEXT NOT NULL);
CREATE TABLE itemTypesCombined (itemTypeID INT NOT NULL PRIMARY KEY, typeName TEXT NOT NULL);
CREATE TABLE fieldsCombined (fieldID INT NOT NULL PRIMARY KEY, fieldName TEXT NOT NULL);
CREATE TABLE baseFieldMappingsCombined (itemTypeID INT, baseFieldID INT, fieldID INT);
CREATE TABLE creatorTypes (creatorTypeID INTEGER PRIMARY KEY, creatorType TEXT);
CREATE TABLE itemTypeCreatorTypes (itemTypeID INT, creatorTypeID INT, primaryField INT);
CREATE TABLE items (itemID INTEGER PRIMARY KEY, itemTypeID INT NOT NULL, dateAdded TIMESTAMP NOT NULL,
  dateModified TIMESTAMP NOT NULL, libraryID INT NOT NULL, key TEXT NOT NULL, version INT NOT NULL DEFAULT 0,
  UNIQUE (libraryID, key));
CREATE TABLE itemDataValues (valueID INTEGER PRIMARY KEY, value UNIQUE);
CREATE TABLE itemData (itemID INT, fieldID INT, valueID, PRIMARY KEY (itemID, fieldID));
CREATE TABLE creators (creatorID INTEGER PRIMARY KEY, firstName TEXT, lastName TEXT, fieldMode INT);
CREATE TABLE itemCreators (itemID INT NOT NULL, creatorID INT NOT NULL, creatorTypeID INT NOT NULL DEFAULT 1,
  orderIndex INT NOT NULL DEFAULT 0);
CREATE TABLE tags (tagID INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE);
CREATE TABLE itemTags (itemID INT NOT NULL, tagID INT NOT NULL, type INT NOT NULL, PRIMARY KEY (itemID, tagID));
CREATE TABLE itemAttachments (itemID INTEGER PRIMARY KEY, parentItemID INT, linkMode INT, contentType TEXT);
CREATE TABLE deletedItems (itemID INTEGER PRIMARY KEY);
INSERT INTO libraries VALUES (1, 'user', 1), (2, 'group', 1), (3, 'feed', 1);
INSERT INTO groups VALUES (7, 2, 'Synthetic group');
INSERT INTO itemTypesCombined VALUES (1, 'journalArticle'), (2, 'book'), (3, 'note'), (4, 'attachment');
INSERT INTO fieldsCombined VALUES (1, 'title'), (2, 'date'), (3, 'publicationTitle'), (4, 'extra'), (5, 'url'),
  (6, 'citationKey');
INSERT INTO creatorTypes VALUES (1, 'author'), (2, 'editor');
INSERT INTO itemTypeCreatorTypes VALUES (1, 1, 1), (2, 1, 1);
";

fn global_schema() -> Vec<u8> {
    let json = r#"{"version": 1, "meta": {"fields": {"date": {"type": "date"}}},
      "csl": {"types": {"article-journal": ["journalArticle"], "book": ["book"], "document": ["note", "attachment"]},
              "fields": {"text": {"title": ["title"], "container-title": ["publicationTitle"], "note": ["extra"],
                                  "URL": ["url"], "citation-key": ["citationKey"]},
                         "date": {"issued": "date"}},
              "names": {"author": "author", "editor": "editor"}}}"#;
    miniz_oxide::deflate::compress_to_vec_zlib(json.as_bytes(), 6)
}

struct Fixture {
    db: Connection,
    values: BTreeMap<String, i64>,
}

impl Fixture {
    fn new(path: &Path, userdata: i64) -> Fixture {
        let db = Connection::open(path).unwrap_or_else(|e| panic!("{e}"));
        db.execute_batch(LAYOUT).unwrap_or_else(|e| panic!("{e}"));
        db.execute("INSERT INTO version VALUES ('userdata', ?1)", [userdata])
            .unwrap_or_else(|e| panic!("{e}"));
        db.execute(
            "INSERT INTO settings VALUES ('globalSchema', 'data', ?1)",
            [global_schema()],
        )
        .unwrap_or_else(|e| panic!("{e}"));
        Fixture {
            db,
            values: BTreeMap::new(),
        }
    }

    fn item(&mut self, id: i64, library: i64, kind: i64, key: &str, added: &str, fields: &[(i64, &str)]) {
        self.db
            .execute(
                "INSERT INTO items VALUES (?1, ?2, ?3, ?3, ?4, ?5, 1)",
                params![id, kind, added, library, key],
            )
            .unwrap_or_else(|e| panic!("{e}"));
        for (field, value) in fields {
            let next = self.values.len() as i64 + 1;
            let value_id = *self.values.entry(value.to_string()).or_insert(next);
            self.db
                .execute(
                    "INSERT OR IGNORE INTO itemDataValues VALUES (?1, ?2)",
                    params![value_id, value],
                )
                .unwrap_or_else(|e| panic!("{e}"));
            self.db
                .execute("INSERT INTO itemData VALUES (?1, ?2, ?3)", params![id, field, value_id])
                .unwrap_or_else(|e| panic!("{e}"));
        }
    }

    fn sql(&self, sql: &str) {
        self.db.execute_batch(sql).unwrap_or_else(|e| panic!("{e}"));
    }
}

/// A personal library with an article (two authors, tags, a PDF and a web
/// snapshot), a trashed book, a note and a trashed PDF; a group with a book;
/// a feed item. The last article is committed only to the WAL, as a Zotero
/// that quit without shutting down cleanly leaves it.
fn library(dir: &Path) {
    let path = dir.join("zotero.sqlite");
    let mut f = Fixture::new(&path, 130);
    f.item(
        1,
        1,
        1,
        "ARTICLE1",
        "2024-01-01 10:00:00",
        &[
            (1, "Sleep spindles in synthetic subjects"),
            (2, "2021-03-14 March 14, 2021"),
            (3, "Journal of Synthetic Studies"),
            (4, "Citation Key: example2021"),
            (5, "https://example.org/a"),
        ],
    );
    f.sql(
        "INSERT INTO creators VALUES (1, 'Ada', 'Example', 0), (2, 'Vincent', 'van Gogh', 0), (3, '', 'Synthetic Institute', 1);
         INSERT INTO itemCreators VALUES (1, 2, 1, 1), (1, 1, 1, 0);
         INSERT INTO tags VALUES (1, 'sleep'), (2, 'memory');
         INSERT INTO itemTags VALUES (1, 2, 1), (1, 1, 0);",
    );
    // A web snapshot whose URL is the item's, a PDF without one, a trashed
    // newer PDF and a linked URL: the PDF wins.
    f.item(
        10,
        1,
        4,
        "SNAPSHOT",
        "2024-01-01 10:01:00",
        &[(5, "https://example.org/a")],
    );
    f.item(11, 1, 4, "PDF00001", "2024-01-01 10:02:00", &[]);
    f.item(12, 1, 4, "PDFTRASH", "2024-01-01 09:00:00", &[]);
    f.item(13, 1, 4, "LINKURL1", "2024-01-01 08:00:00", &[]);
    f.sql(
        "INSERT INTO itemAttachments VALUES (10, 1, 1, 'text/html'), (11, 1, 0, 'application/pdf'),
           (12, 1, 0, 'application/pdf'), (13, 1, 3, 'application/pdf');
         INSERT INTO deletedItems VALUES (12);",
    );
    f.item(2, 1, 2, "TRASHED1", "2024-01-02 10:00:00", &[(1, "A trashed book")]);
    f.sql("INSERT INTO deletedItems VALUES (2);");
    f.item(3, 1, 3, "NOTE0001", "2024-01-03 10:00:00", &[]);
    f.item(
        4,
        2,
        2,
        "GROUPBK1",
        "2024-01-04 10:00:00",
        &[(1, "A group book"), (2, "0000-00-00 Spring term")],
    );
    f.sql("INSERT INTO itemCreators VALUES (4, 3, 1, 0);");
    f.item(5, 3, 1, "FEEDITEM", "2024-01-05 10:00:00", &[(1, "A feed item")]);
    f.sql("PRAGMA journal_mode = WAL; PRAGMA wal_autocheckpoint = 0;");
    f.item(
        6,
        1,
        2,
        "WALONLY1",
        "2024-01-06 10:00:00",
        &[(1, "Committed only to the WAL"), (6, "walonly")],
    );
    // Copy the files while the writer is still open, as a crash leaves them.
    let crashed = dir.join("crashed");
    std::fs::create_dir(&crashed).unwrap_or_else(|e| panic!("{e}"));
    for name in ["zotero.sqlite", "zotero.sqlite-wal", "zotero.sqlite-shm"] {
        std::fs::copy(dir.join(name), crashed.join(name)).unwrap_or_else(|e| panic!("{e}"));
    }
    let wal = std::fs::metadata(crashed.join("zotero.sqlite-wal")).map(|m| m.len());
    assert!(wal.is_ok_and(|len| len > 0), "the last item is only in the WAL");
    drop(f);
    for name in ["zotero.sqlite", "zotero.sqlite-wal", "zotero.sqlite-shm"] {
        let _ = std::fs::remove_file(dir.join(name));
        std::fs::rename(crashed.join(name), dir.join(name)).unwrap_or_else(|e| panic!("{e}"));
    }
    std::fs::remove_dir(crashed).unwrap_or_else(|e| panic!("{e}"));
}

/// Every file in `dir` with its bytes.
fn contents(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{e}"))
        .map(|entry| {
            let entry = entry.unwrap_or_else(|e| panic!("{e}"));
            let bytes = std::fs::read(entry.path()).unwrap_or_default();
            (entry.file_name().to_string_lossy().into_owned(), bytes)
        })
        .collect()
}

fn temp() -> (tempfile::TempDir, tempfile::TempDir) {
    let make = || tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    (make(), make())
}

fn loaded(outcome: Result<Refresh, SnapshotError>) -> (LoadResult, Fingerprint) {
    match outcome {
        Ok(Refresh::Loaded { result, fingerprint }) => (result, fingerprint),
        other => panic!("expected a loaded library, got {other:?}"),
    }
}

#[test]
fn a_closed_library_is_read_from_a_copy_without_touching_zoteros_files() {
    let (data, scratch) = temp();
    library(data.path());
    assert!(data.path().join("zotero.sqlite-wal").exists());
    let before = contents(data.path());
    let (result, fingerprint) = loaded(read(
        data.path(),
        scratch.path(),
        &[Library::User, Library::Group(7), Library::Group(8)],
        None,
    ));
    assert_eq!(contents(data.path()), before, "Zotero's files are untouched");
    assert_eq!(
        std::fs::read_dir(scratch.path()).map(Iterator::count).ok(),
        Some(0),
        "the copy is removed"
    );

    let keys: Vec<_> = result.references.iter().map(|r| r.id.native_key.as_str()).collect();
    assert_eq!(
        keys,
        ["ARTICLE1", "WALONLY1", "GROUPBK1"],
        "no trash, notes or feeds; the WAL's item is read"
    );
    assert_eq!(result.warnings.len(), 1, "{:?}", result.warnings);
    assert!(result.warnings[0].message.starts_with("Zotero group 8 was skipped"));

    let article = &result.references[0];
    assert_eq!(article.id.source, "zotero:user");
    assert_eq!(article.citekey.as_deref(), Some("example2021"));
    let families: Vec<_> = article.authors.iter().filter_map(|a| a.family.as_deref()).collect();
    assert_eq!(families, ["Example", "Gogh"], "in Zotero's order, particles split");
    assert_eq!(article.issued.as_ref().map(|i| i.start.year), Some(2021));
    assert_eq!(article.keywords, ["sleep", "memory"]);
    assert_eq!(article.attachment, Some(ibis_core::model::AttachmentKind::Pdf));
    assert_eq!(result.references[1].citekey.as_deref(), Some("walonly"));
    let group = &result.references[2];
    assert_eq!(group.id.source, "zotero:group:7");
    assert_eq!(group.authors[0].literal.as_deref(), Some("Synthetic Institute"));
    assert_eq!(group.variables.get("issued").map(String::as_str), Some("Spring term"));

    assert_eq!(
        read(data.path(), scratch.path(), &[Library::User], Some(&fingerprint)),
        Ok(Refresh::Unchanged)
    );
}

#[test]
fn better_bibtex_keys_count_only_for_their_own_item() {
    let (data, scratch) = temp();
    library(data.path());
    let bbt = Connection::open(data.path().join("better-bibtex.sqlite")).unwrap_or_else(|e| panic!("{e}"));
    bbt.execute_batch(
        "CREATE TABLE citationkey (itemID PRIMARY KEY, itemKey NOT NULL, libraryID NOT NULL, citationKey NOT NULL, pinned);
         INSERT INTO citationkey VALUES (1, 'ARTICLE1', 1, 'bbtarticle', 0), (4, 'GROUPBK1', 2, 'bbtgroup', 0),
           (6, 'OTHERKEY', 1, 'mismatched', 0);",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    drop(bbt);
    let (result, _) = loaded(read(
        data.path(),
        scratch.path(),
        &[Library::User, Library::Group(7)],
        None,
    ));
    let keys: Vec<_> = result.references.iter().map(|r| r.citekey.as_deref()).collect();
    // Extra comes before Better BibTeX; Zotero's own field before both.
    assert_eq!(keys, [Some("example2021"), Some("walonly"), Some("bbtgroup")]);
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
}

#[test]
fn unknown_zotero_versions_and_damaged_files_are_refused_clearly() {
    for (version, expected) in [(122, SnapshotError::TooOld), (131, SnapshotError::TooNew)] {
        let (data, scratch) = temp();
        drop(Fixture::new(&data.path().join("zotero.sqlite"), version));
        assert_eq!(read(data.path(), scratch.path(), &[Library::User], None), Err(expected));
    }
    let (data, scratch) = temp();
    assert_eq!(
        read(data.path(), scratch.path(), &[Library::User], None),
        Err(SnapshotError::NotFound)
    );
    std::fs::write(data.path().join("zotero.sqlite"), vec![0x5a; 8192]).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        read(data.path(), scratch.path(), &[Library::User], None),
        Err(SnapshotError::Damaged)
    );
    std::fs::write(data.path().join("zotero.sqlite"), b"").unwrap_or_else(|e| panic!("{e}"));
    assert!(read(data.path(), scratch.path(), &[Library::User], None).is_err());
    assert_eq!(std::fs::read_dir(scratch.path()).map(Iterator::count).ok(), Some(0));
}

#[test]
fn a_database_missing_a_column_ibis_reads_is_refused() {
    let (data, scratch) = temp();
    let f = Fixture::new(&data.path().join("zotero.sqlite"), 125);
    f.sql("ALTER TABLE itemTags DROP COLUMN type;");
    drop(f);
    assert_eq!(
        read(data.path(), scratch.path(), &[Library::User], None),
        Err(SnapshotError::TooNew)
    );
}

#[cfg(unix)]
#[test]
fn a_linked_database_is_not_followed() {
    let (data, scratch) = temp();
    let elsewhere = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    library(elsewhere.path());
    std::os::unix::fs::symlink(
        elsewhere.path().join("zotero.sqlite"),
        data.path().join("zotero.sqlite"),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        read(data.path(), scratch.path(), &[Library::User], None),
        Err(SnapshotError::Unreadable)
    );
}

#[cfg(unix)]
#[test]
fn copies_left_by_a_stopped_read_are_removed_later() {
    let (data, scratch) = temp();
    library(data.path());
    let old = scratch.path().join("ibis-zotero-1-0");
    let other = scratch.path().join("someone-elses");
    for dir in [&old, &other] {
        std::fs::create_dir(dir).unwrap_or_else(|e| panic!("{e}"));
        let hour_ago = SystemTime::now() - Duration::from_secs(3600);
        File::open(dir)
            .and_then(|d| d.set_modified(hour_ago))
            .unwrap_or_else(|e| panic!("{e}"));
    }
    loaded(read(data.path(), scratch.path(), &[Library::User], None));
    assert!(!old.exists());
    assert!(other.exists());
}

#[test]
fn cloud_synced_data_folders_are_recognised() {
    for synced in [
        "/Users/x/Dropbox/Zotero",
        "/Users/x/Library/CloudStorage/GoogleDrive-x/My Drive/Zotero",
        "/Users/x/Library/Mobile Documents/com~apple~CloudDocs/Zotero",
        "C:/Users/x/OneDrive - Example/Zotero",
        "/home/x/Nextcloud/Zotero",
    ] {
        assert!(cloud_synced(Path::new(synced)), "{synced}");
    }
    for local in ["/Users/x/Zotero", "/home/x/Documents/Zotero", "C:/Users/x/Zotero"] {
        assert!(!cloud_synced(Path::new(local)), "{local}");
    }
}
