use crate::closed::tests::{control, edit, fixture, read_fixture};
use crate::closed::{ClosedSnapshot, private_copy::Store};
use std::path::Path;

fn last_run(dir: &Path, version: &str) {
    edit(
        dir,
        &format!(
            "CREATE TABLE settings(setting TEXT,key TEXT,value,PRIMARY KEY(setting,key));
             INSERT INTO settings VALUES('client','lastVersion','{version}');"
        ),
    );
}

fn store(dir: &Path, sql: &str) {
    let db = rusqlite::Connection::open(dir.join("better-bibtex.sqlite"))
        .unwrap_or_else(|_| panic!("synthetic Better BibTeX store"));
    db.execute_batch(sql).unwrap_or_else(|_| panic!("synthetic store SQL"));
}

const KEYS: &str = "CREATE TABLE citationkey (itemID INTEGER PRIMARY KEY, itemKey TEXT, libraryID INTEGER,
                                              citationKey TEXT, pinned INTEGER);
    INSERT INTO citationkey VALUES (10, 'SYNTH001', 1, 'Store2020', 0), (11, 'SYNTH001', 2, 'Group2020', 0),
                                   (99, 'ABSENT01', 1, 'Absent2020', 0);";

fn read_only(dir: &Path, json: &str) {
    std::fs::create_dir_all(dir.join("better-bibtex")).unwrap_or_else(|_| panic!("synthetic folder"));
    std::fs::write(dir.join("better-bibtex").join("read-only.json"), json).unwrap_or_else(|_| panic!("synthetic keys"));
}

fn keys(snapshot: &ClosedSnapshot) -> Vec<Vec<String>> {
    snapshot
        .libraries
        .iter()
        .flat_map(|library| &library.items)
        .map(|item| item.data.better_bibtex_keys.clone())
        .collect()
}

fn read(dir: &Path) -> ClosedSnapshot {
    read_fixture(dir).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn zotero_7_reads_every_key_from_the_store() {
    let dir = fixture(7);
    last_run(dir.path(), "7.0.15");
    store(dir.path(), KEYS);
    let snapshot = read(dir.path());
    assert_eq!(keys(&snapshot), [vec!["Store2020"], vec!["Group2020"]]);
    assert!(!snapshot.better_bibtex_unknown);
}

#[test]
fn a_store_beside_zotero_8_is_not_served() {
    let dir = fixture(8);
    last_run(dir.path(), "8.0.1");
    store(dir.path(), KEYS);
    let snapshot = read(dir.path());
    assert_eq!(keys(&snapshot), [Vec::<String>::new(), vec![]]);
    assert!(!snapshot.better_bibtex_unknown);
}

#[test]
fn no_better_bibtex_means_no_keys_and_nothing_unknown() {
    let snapshot = read(fixture(10).path());
    assert_eq!(keys(&snapshot), [Vec::<String>::new(), vec![]]);
    assert!(!snapshot.better_bibtex_unknown);
}

#[test]
fn a_store_that_cannot_be_read_leaves_keys_unknown_but_the_library_readable() {
    // No record of which Zotero last ran.
    let dir = fixture(7);
    store(dir.path(), KEYS);
    assert!(read(dir.path()).better_bibtex_unknown);

    // An older store layout.
    let dir = fixture(7);
    last_run(dir.path(), "7.0.15");
    store(dir.path(), "CREATE TABLE \"better-bibtex\" (name TEXT, data TEXT);");
    assert!(read(dir.path()).better_bibtex_unknown);

    // Damaged bytes.
    let dir = fixture(7);
    last_run(dir.path(), "7.0.15");
    std::fs::write(dir.path().join("better-bibtex.sqlite"), b"synthetic damaged store")
        .unwrap_or_else(|_| panic!("synthetic bytes"));
    let snapshot = read(dir.path());
    assert!(snapshot.better_bibtex_unknown);
    assert_eq!(snapshot.libraries[0].items.len(), 1);

    // A hot journal.
    let dir = fixture(7);
    last_run(dir.path(), "7.0.15");
    store(dir.path(), KEYS);
    std::fs::write(
        dir.path().join("better-bibtex.sqlite-journal"),
        b"\xd9\xd5\x05\xf9\x20\xa1\x63\xd7",
    )
    .unwrap_or_else(|_| panic!("synthetic journal"));
    let snapshot = read(dir.path());
    assert!(snapshot.better_bibtex_unknown);
    assert_eq!(keys(&snapshot), [Vec::<String>::new(), vec![]]);
}

#[test]
fn read_only_keys_fill_only_items_without_their_own_key() {
    let dir = fixture(8);
    last_run(dir.path(), "8.0.1");
    read_only(
        dir.path(),
        r#"[{"itemID": 10, "libraryID": 1, "itemKey": "SYNTH001", "citationKey": "Other2020"},
            {"itemID": 11, "libraryID": 2, "itemKey": "SYNTH001", "citationKey": "Shadow2020"}]"#,
    );
    let snapshot = read(dir.path());
    // The personal item has Zotero's own key, which Better BibTeX serves.
    assert_eq!(keys(&snapshot), [vec![], vec!["Shadow2020"]]);
    assert!(!snapshot.better_bibtex_unknown);

    read_only(dir.path(), "{not json");
    let snapshot = read(dir.path());
    assert!(snapshot.better_bibtex_unknown);
    assert_eq!(keys(&snapshot), [Vec::<String>::new(), vec![]]);
}

#[test]
fn a_store_changing_during_the_copy_is_copied_again() {
    let dir = fixture(7);
    last_run(dir.path(), "7.0.15");
    store(dir.path(), KEYS);
    let scratch = tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic scratch"));
    let store_root = Store::open(scratch.path(), &control()).unwrap_or_else(|e| panic!("{e}"));
    let mut attempts = 0;
    let snapshot = store_root
        .read_with(
            dir.path(),
            &control(),
            &mut || {
                attempts += 1;
                if attempts == 1 {
                    store(
                        dir.path(),
                        "UPDATE citationkey SET citationKey='Changed2020' WHERE itemID=10",
                    );
                }
                Ok(())
            },
            &mut || Ok(()),
        )
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(attempts, 2);
    assert_eq!(keys(&snapshot)[0], ["Changed2020"]);
}
