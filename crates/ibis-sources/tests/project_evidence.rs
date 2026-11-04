#![allow(clippy::unwrap_used)]
use ibis_core::contract::*;
use ibis_sources::project::{ProjectRead, WorkMatch};
use serde_json::json;

fn record(doi: Option<&str>) -> Record {
    let mut record = Record {
        id: ItemRef { library: LibraryId::digest(b"invented-project-evidence"), key:"book".into() }, added_at:None,
        csl: serde_json::from_value(json!({"type":"book","title":"Invented Atlas","author":[{"family":"Vale","given":"Ada"}],"issued":{"date-parts":[[2024]]}})).unwrap(),
        citation_keys:vec![], aliases:vec![], warnings:vec![], identifiers:vec![],
    };
    if let Some(doi) = doi {
        record.csl.insert("DOI".into(), json!(doi));
    }
    record
}

const MATCHED: &str = "@book{vale2024,title={Invented Atlas},author={Vale, Ada},year={2024},doi={10.1000/intended}}\n";

#[test]
fn another_bibliography_cannot_borrow_a_previous_files_retained_copy() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first.bib");
    std::fs::write(&first, "@book{vale2024,title={Invented Atlas},year={2024}}\n").unwrap();
    let read = ProjectRead::read(&first, None);
    let second = directory.path().join("unavailable.bib");
    let other = ProjectRead::read(&second, Some(&read));
    assert!(!other.last_good(), "a different selected file has no retained evidence");
}

#[test]
fn same_file_matching_evidence_is_dated_and_a_new_coherent_conflict_supersedes_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    std::fs::write(&path, MATCHED).unwrap();
    let good = ProjectRead::read(&path, None);
    assert!(good.last_checked_ms().is_some());
    std::fs::remove_file(&path).unwrap();
    let failed = ProjectRead::read(&path, Some(&good));
    assert_eq!(
        failed.work_match("vale2024", &record(Some("https://doi.org/10.1000/INTENDED"))),
        WorkMatch::Matched
    );
    assert_eq!(failed.last_checked_ms(), good.last_checked_ms());
    assert!(!format!("{failed:?}").contains(directory.path().to_str().unwrap()));
    std::fs::write(&path, MATCHED.replace("10.1000/intended", "10.1000/other")).unwrap();
    let conflict = ProjectRead::read(&path, Some(&failed));
    assert_eq!(
        conflict.work_match("vale2024", &record(Some("10.1000/intended"))),
        WorkMatch::Conflict
    );
    std::fs::remove_file(&path).unwrap();
    let failed_again = ProjectRead::read(&path, Some(&conflict));
    assert_eq!(
        failed_again.work_match("vale2024", &record(Some("10.1000/intended"))),
        WorkMatch::Conflict
    );
}

#[test]
fn field_drift_is_uncertain_and_key_presence_alone_is_not_a_work_match() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    std::fs::write(
        &path,
        "@book{vale2024,title={Another edition},author={Vale, Ada},year={2025}}\n",
    )
    .unwrap();
    assert_eq!(
        ProjectRead::read(&path, None).work_match("vale2024", &record(None)),
        WorkMatch::Uncertain
    );
    std::fs::write(&path, "@book{vale2024,title={Invented Atlas}}\n").unwrap();
    assert_eq!(
        ProjectRead::read(&path, None).work_match("vale2024", &record(None)),
        WorkMatch::Uncertain
    );
    std::fs::write(
        &path,
        "@book{vale2024,title={Invented Atlas},author={Vale, Ada},year={2024}}\n",
    )
    .unwrap();
    assert_eq!(
        ProjectRead::read(&path, None).work_match("vale2024", &record(None)),
        WorkMatch::Matched
    );
}

#[test]
fn incomplete_newer_conflicts_survive_failure_and_only_a_coherent_read_can_clear_them() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    std::fs::write(&path, MATCHED).unwrap();
    let good = ProjectRead::read(&path, None);
    let broken = "@book{broken,title={Never closed\n";
    std::fs::write(&path, format!("{MATCHED}{broken}")).unwrap();
    let incomplete = ProjectRead::read(&path, Some(&good));
    assert_eq!(
        incomplete.work_match("vale2024", &record(Some("10.1000/intended"))),
        WorkMatch::Matched
    );
    std::fs::write(
        &path,
        format!("{}{broken}", MATCHED.replace("10.1000/intended", "10.1000/other")),
    )
    .unwrap();
    let conflict = ProjectRead::read(&path, Some(&incomplete));
    assert_eq!(
        conflict.work_match("vale2024", &record(Some("10.1000/intended"))),
        WorkMatch::Conflict
    );
    assert!(
        !conflict.same_answers(&incomplete),
        "same damage lines cannot hide new identity conflicts"
    );
    std::fs::remove_file(&path).unwrap();
    let unavailable = ProjectRead::read(&path, Some(&conflict));
    assert_eq!(
        unavailable.work_match("vale2024", &record(Some("10.1000/intended"))),
        WorkMatch::Conflict
    );
    std::fs::write(&path, format!("{MATCHED}{broken}")).unwrap();
    let still_incomplete = ProjectRead::read(&path, Some(&unavailable));
    assert_eq!(
        still_incomplete.work_match("vale2024", &record(Some("10.1000/intended"))),
        WorkMatch::Conflict
    );
    std::fs::write(&path, MATCHED).unwrap();
    let recovered = ProjectRead::read(&path, Some(&still_incomplete));
    assert_eq!(
        recovered.work_match("vale2024", &record(Some("10.1000/intended"))),
        WorkMatch::Matched
    );
}

#[test]
fn duplicate_partial_entries_cannot_hide_conflicting_identity_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    let other = MATCHED.replace("10.1000/intended", "10.1000/other");
    for (first, second) in [(MATCHED, other.as_str()), (other.as_str(), MATCHED)] {
        std::fs::write(&path, MATCHED).unwrap();
        let good = ProjectRead::read(&path, None);
        std::fs::write(&path, format!("{first}{second}@book{{broken,title={{Never closed\n")).unwrap();
        let duplicated = ProjectRead::read(&path, Some(&good));
        assert!(
            duplicated.evidence_limited(),
            "deduplication must not erase possible observed conflicts"
        );
        assert_ne!(
            duplicated.work_match("vale2024", &record(Some("10.1000/intended"))),
            WorkMatch::Matched
        );
        std::fs::remove_file(&path).unwrap();
        assert!(ProjectRead::read(&path, Some(&duplicated)).evidence_limited());
    }
}

#[test]
fn retained_observation_bytes_are_bounded_before_a_large_key_can_be_kept() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    std::fs::write(&path, MATCHED).unwrap();
    let good = ProjectRead::read(&path, None);
    let key = "k".repeat(256 * 1024 + 1);
    std::fs::write(
        &path,
        format!("@book{{{key},doi={{10.1000/other}}}}\n@book{{broken,title={{Never closed\n"),
    )
    .unwrap();
    let limited = ProjectRead::read(&path, Some(&good));
    assert!(limited.evidence_limited());
    std::fs::remove_file(&path).unwrap();
    assert!(ProjectRead::read(&path, Some(&limited)).evidence_limited());
    std::fs::write(&path, MATCHED).unwrap();
    assert!(!ProjectRead::read(&path, Some(&limited)).evidence_limited());
}

#[test]
fn a_first_incomplete_read_is_never_labelled_a_last_good_copy_after_failure() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    std::fs::write(&path, format!("{MATCHED}@book{{broken,title={{Never closed\n")).unwrap();
    let incomplete = ProjectRead::read(&path, None);
    std::fs::remove_file(&path).unwrap();
    let failed = ProjectRead::read(&path, Some(&incomplete));
    assert!(!failed.last_good());
    assert_eq!(failed.last_checked_ms(), None);
}

#[test]
fn complete_duplicate_keys_cannot_clear_a_previously_observed_work_conflict() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    let other = MATCHED.replace("10.1000/intended", "10.1000/other");
    for (first, second) in [(MATCHED, other.as_str()), (other.as_str(), MATCHED)] {
        std::fs::write(&path, MATCHED).unwrap();
        let good = ProjectRead::read(&path, None);
        std::fs::write(&path, format!("{other}@book{{broken,title={{Never closed\n")).unwrap();
        let known_conflict = ProjectRead::read(&path, Some(&good));
        std::fs::write(&path, format!("{first}{second}")).unwrap();
        let duplicates = ProjectRead::read(&path, Some(&known_conflict));
        assert_eq!(
            duplicates.work_match("vale2024", &record(Some("10.1000/intended"))),
            WorkMatch::Conflict
        );
        std::fs::remove_file(&path).unwrap();
        let missing = ProjectRead::read(&path, Some(&duplicates));
        assert_eq!(
            missing.work_match("vale2024", &record(Some("10.1000/intended"))),
            WorkMatch::Conflict
        );
        std::fs::write(&path, MATCHED).unwrap();
        assert_eq!(
            ProjectRead::read(&path, Some(&missing)).work_match("vale2024", &record(Some("10.1000/intended"))),
            WorkMatch::Matched
        );
    }
}
