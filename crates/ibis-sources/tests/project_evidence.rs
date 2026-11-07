#![allow(clippy::unwrap_used)]
use ibis_core::contract::*;
use ibis_sources::project::{KeyCheck, ProjectRead};
use serde_json::json;

fn record(doi: &str) -> Record {
    let mut record = Record {
        id: ItemRef { library: LibraryId::digest(b"invented-project-evidence"), key:"book".into() }, added_at:None,
        csl: serde_json::from_value(json!({"type":"book","title":"Invented Atlas","author":[{"family":"Vale","given":"Ada"}],"issued":{"date-parts":[[2024]]}})).unwrap(),
        citation_keys:vec![], aliases:vec![], warnings:vec![], identifiers:vec![],
    };
    record.csl.insert("DOI".into(), json!(doi));
    record
}

const MATCHED: &str = "@book{vale2024,title={Invented Atlas},author={Vale, Ada},year={2024},doi={10.1000/intended}}\n";

#[test]
fn a_read_that_fails_after_a_good_one_checks_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    std::fs::write(&path, MATCHED).unwrap();
    let good = ProjectRead::read(&path, None);
    assert_eq!(good.check("vale2024", None), KeyCheck::In { line: 1 });
    std::fs::remove_file(&path).unwrap();
    let failed = ProjectRead::read(&path, Some(&good));
    assert_eq!(failed.check("vale2024", None), KeyCheck::CouldNotCheck);
    assert!(!failed.conflicts("vale2024", &record("10.1000/other")));
    assert!(!format!("{failed:?}").contains(directory.path().to_str().unwrap()));
}

#[test]
fn a_different_doi_in_the_current_read_is_a_conflict_even_in_a_damaged_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    std::fs::write(&path, MATCHED).unwrap();
    let read = ProjectRead::read(&path, None);
    assert!(!read.conflicts("vale2024", &record("https://doi.org/10.1000/INTENDED")));
    assert!(read.conflicts("vale2024", &record("10.1000/other")));

    std::fs::write(&path, format!("{MATCHED}@book{{broken,title={{Never closed\n")).unwrap();
    let damaged = ProjectRead::read(&path, Some(&read));
    assert!(damaged.conflicts("vale2024", &record("10.1000/other")));
    assert!(!damaged.conflicts("vale2024", &record("10.1000/intended")));
}

#[test]
fn a_repeated_key_whose_entries_name_another_work_is_a_conflict() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    let other = MATCHED.replace("10.1000/intended", "10.1000/other");
    std::fs::write(&path, format!("{other}{other}")).unwrap();
    let read = ProjectRead::read(&path, None);
    assert_eq!(read.check("vale2024", None), KeyCheck::Repeated { lines: vec![1, 2] });
    assert!(read.conflicts("vale2024", &record("10.1000/intended")));
    std::fs::write(&path, format!("{MATCHED}{MATCHED}")).unwrap();
    assert!(!ProjectRead::read(&path, None).conflicts("vale2024", &record("10.1000/intended")));
}
