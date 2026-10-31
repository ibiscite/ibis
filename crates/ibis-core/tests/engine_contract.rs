//! Acceptance checks for the engine contract scaffolding (ADR 0028).
use std::path::Path;

#[test]
fn engine_crates_and_golden_cases_exist() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for name in ["ibis-sources", "ibis-csl", "ibis-engine"] {
        assert!(
            root.join("crates").join(name).join("Cargo.toml").is_file(),
            "missing {name}"
        );
    }
    let cases = std::fs::read_dir(root.join("fixtures/cases"))?;
    assert_eq!(
        cases
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            .count(),
        11
    );
    Ok(())
}

#[test]
fn generated_schema_matches_checked_in_contract() -> Result<(), Box<dyn std::error::Error>> {
    let checked: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/engine-contract.schema.json"))?;
    assert_eq!(
        serde_json::to_value(ibis_core::contract::schema())?,
        checked,
        "regenerate schema after an intentional contract change"
    );
    Ok(())
}

#[test]
fn golden_cases_parse_and_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    use ibis_core::contract::GoldenCase;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cases");
    let mut names = std::collections::BTreeSet::new();
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let wire = std::fs::read_to_string(path)?;
        let case: GoldenCase = serde_json::from_str(&wire)?;
        assert!(names.insert(case.name.clone()));
        if let Some(result) = case.expected.data() {
            assert!(result.content_matches_hash(), "{}: hash", case.name);
            assert_eq!(
                result.outcomes.len(),
                case.request.items.len(),
                "{}: outcomes",
                case.name
            );
        }
        assert_eq!(
            serde_json::from_str::<GoldenCase>(&serde_json::to_string(&case)?)?,
            case
        );
    }
    assert_eq!(names.len(), 11);
    Ok(())
}

#[test]
fn deep_snapshot_rejection_is_safe() -> Result<(), Box<dyn std::error::Error>> {
    const CHILD: &str = "IBIS_DEEP_SNAPSHOT_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        std::thread::spawn(|| {
            use ibis_core::contract::*;
            use std::collections::BTreeMap;
            for mode in [0, 2, 3] {
                let mut value = serde_json::Value::Null;
                for _ in 0..20_000 {
                    value = serde_json::Value::Array(vec![value]);
                }
                let mut record = Record {
                    added_at: None,
                    id: ItemRef {
                        library: LibraryId::file(b"synthetic-depth-fixture"),
                        key: if mode == 3 { "".into() } else { "work".into() },
                    },
                    csl: BTreeMap::new(),
                    citation_keys: vec![],
                    aliases: vec![],
                    identifiers: vec![],
                };
                record.csl.insert("title".into(), value);
                let mut objects = vec![LibraryObject::Record(record)];
                if mode == 2 {
                    objects.push(LibraryObject::Record(Record {
                        added_at: None,
                        id: ItemRef {
                            library: LibraryId::file(b"synthetic-depth-fixture"),
                            key: "work".into(),
                        },
                        csl: BTreeMap::new(),
                        citation_keys: vec![],
                        aliases: vec![],
                        identifiers: vec![],
                    }));
                }
                assert!(Snapshot::new(objects).is_err());
            }
        })
        .join()
        .map_err(|_| "snapshot worker panicked")?;
        return Ok(());
    }
    let status = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", "deep_snapshot_rejection_is_safe", "--nocapture"])
        .env(CHILD, "1")
        .status()?;
    assert!(status.success(), "deep snapshot rejection aborted the process");
    Ok(())
}

#[test]
fn empty_citation_cannot_be_copied() {
    use ibis_core::contract::*;
    for text in ["", " \t\n", "\u{2003}"] {
        let result = CitationResult::new(
            text.into(),
            None,
            None,
            DisplayTree::default(),
            vec![ItemOutcome::Rendered { item: 0 }],
            vec![],
            vec![],
        );
        assert!(result.copy_content(1).is_none());
    }
}
