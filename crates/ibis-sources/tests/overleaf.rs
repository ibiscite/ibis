//! A person selects local bibliography files, including files they extracted
//! from an explicit Overleaf source download. No hosted action is exercised.
#![allow(clippy::unwrap_used)]
#[path = "support/bib.rs"]
mod bib;

use ibis_sources::project::{Doubt, KeyCheck, ProjectProblem, ProjectRead};
use ibis_sources::{SourceError, WarningCode, load_file, reload_file_snapshot};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/overleaf")
        .join(name)
}

#[test]
fn selected_files_match_pandoc_on_citable_fields() {
    let oracle: Value = serde_json::from_str(include_str!("../../../fixtures/overleaf/oracle.json")).unwrap();
    let mut libraries = Vec::new();
    for name in ["references.bib", "supplement.bib"] {
        let loaded = load_file(&fixture(name)).unwrap();
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        let expected = oracle["inputs"][name]["records"].as_array().unwrap();
        let mut expected_keys: Vec<_> = expected.iter().map(|item| item["id"].as_str().unwrap()).collect();
        expected_keys.sort_unstable();
        assert_eq!(bib::keys(&loaded), expected_keys, "{name}");
        for item in expected {
            // Pandoc's id is the BibTeX key; Ibis also publishes it as
            // citation-key. Compare the semantic fields without stripping markup
            // or case, and compare absence as well as presence.
            let key = item["id"].as_str().unwrap();
            let record = bib::record(&loaded, key);
            assert_eq!(record.citation_key().unwrap(), Some(key));
            for field in ["type", "title", "author", "issued", "DOI", "ISBN", "container-title"] {
                assert_eq!(record.csl.get(field), item.get(field), "{name} {key} {field}");
            }
        }
        libraries.push(loaded);
    }
    assert_ne!(libraries[0].library, libraries[1].library);
}

#[test]
fn duplicate_keys_are_never_selectable_even_if_identical() {
    let entry = "@book{duplicate,title={One},doi={10.1000/one}}\n";
    for second in [entry.to_owned(), entry.replace("10.1000/one", "10.1000/two")] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("download.bib");
        std::fs::write(&path, format!("{entry}{second}@book{{safe,title={{Safe}}}}\n")).unwrap();
        let loaded = load_file(&path).unwrap();
        assert_eq!(bib::keys(&loaded), ["safe"]);
        assert_eq!(bib::with_code(&loaded, WarningCode::DuplicateKey).len(), 2);
        assert_eq!(
            ProjectRead::read(&path, None).check("duplicate", None),
            KeyCheck::Repeated { lines: vec![1, 2] }
        );
    }
}

#[test]
fn current_conflicts_and_incomplete_or_missing_downloads_are_distinct() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("download.bib");
    std::fs::copy(fixture("references.bib"), &path).unwrap();
    let loaded = load_file(&path).unwrap();
    let original = bib::record(&loaded, "Vale2024");
    let changed = std::fs::read_to_string(&path)
        .unwrap()
        .replace("10.1000/imaginary-rivers", "10.1000/another-work");
    std::fs::write(&path, &changed).unwrap();
    let current = ProjectRead::read(&path, None);
    assert!(current.conflicts("Vale2024", original));
    assert_eq!(
        current.check("Vale2024", Some(original)),
        KeyCheck::MayNameAnother {
            line: 1,
            doubt: Doubt::Fields
        }
    );

    std::fs::write(&path, format!("{changed}\n@book{{broken,title={{Unclosed\n")).unwrap();
    let damaged = load_file(&path).unwrap();
    assert!(!damaged.warnings.is_empty());
    let incomplete = ProjectRead::read(&path, Some(&current));
    assert!(!incomplete.damaged_lines().is_empty());
    assert_eq!(incomplete.check("absent", None), KeyCheck::CouldNotCheck);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(load_file(&path).err(), Some(SourceError::Missing));
    let missing = ProjectRead::read(&path, Some(&incomplete));
    assert_eq!(missing.problem(), Some(ProjectProblem::Unavailable));
    assert_eq!(missing.check("absent", None), KeyCheck::CouldNotCheck);
    std::fs::write(&path, "").unwrap();
    assert!(bib::records(&load_file(&path).unwrap()).is_empty());
    assert_eq!(ProjectRead::read(&path, None).check("absent", None), KeyCheck::Missing);
}

#[test]
fn replacement_reload_keeps_the_earlier_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("download.bib");
    std::fs::copy(fixture("references.bib"), &path).unwrap();
    let old = load_file(&path).unwrap();
    let held_snapshot = old.snapshot.clone();
    std::fs::copy(fixture("supplement.bib"), &path).unwrap();
    let (new, _) = reload_file_snapshot(&path, Some(&old.snapshot), &|| false).unwrap();
    assert_eq!(old.library, new.library);
    assert_eq!(bib::keys(&old), ["North2021", "Vale2024"]);
    assert_eq!(old.snapshot, held_snapshot);
    assert_eq!(bib::keys(&new), ["Field2020"]);
    assert_eq!(
        bib::text(&new, "Field2020", "title"),
        Some("Notes on fictional landscapes")
    );
    assert_ne!(old.snapshot.fingerprints(), new.snapshot.fingerprints());
}

#[cfg(unix)]
#[test]
fn reading_a_read_only_download_preserves_bytes_and_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("download.bib");
    std::fs::copy(fixture("references.bib"), &path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
    let before = std::fs::read(&path).unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    let loaded = load_file(&path).unwrap();
    assert_eq!(bib::keys(&loaded), ["North2021", "Vale2024"]);
    assert_eq!(
        ProjectRead::read(&path, None).check("Vale2024", None),
        KeyCheck::In { line: 1 }
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode(), mode);
}
