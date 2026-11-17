//! The bundled data uses the production CSL-JSON reader and citation receipts.
use super::engine_library::FileLibrary;
use super::*;
use ibis_core::contract::DraftItem;
use ibis_engine::Cancellation;
use ibis_engine::library::{Current, Owner};
use serde_json::json;
use std::path::PathBuf;

fn source() -> Source {
    Source::Samples(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/samples.csljson"))
}

fn loaded(source: Source) -> Owner {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let owner = super::tests::owner(crate::preferences::SettingsFile::new(
        directory.path().join("settings.json"),
    ));
    super::tests::open(&owner, source);
    owner
}

fn preview(file: &Current, term: &str) -> ibis_core::contract::Preview {
    preview_output(file, term, "apa")
}

fn preview_output(file: &Current, term: &str, output: &str) -> ibis_core::contract::Preview {
    let query =
        serde_json::from_value(json!({"text":term,"interpret_locators":false})).unwrap_or_else(|e| panic!("{e}"));
    let found = file
        .search(&query, 10, None, &Cancellation::default())
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(found.results.len(), 1);
    let row = &found.results[0];
    crate::test_citations::preview(
        &file.engine,
        file.sample(),
        vec![DraftItem {
            prefix: None,
            suffix: None,
            id: row.id.clone(),
            cite: serde_json::from_value(json!(row.cite)).unwrap_or_else(|e| panic!("{e}")),
            locator: None,
            locator_input: None,
            form: None,
        }],
        output,
    )
    .unwrap_or_else(|_| panic!("preview"))
}

#[test]
fn bundled_samples_load_through_csl_json_and_copy_marked_apa() {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let broken = directory.path().join("malformed.csljson");
    std::fs::write(&broken, "[{\"id\":").unwrap_or_else(|e| panic!("{e}"));
    for path in [broken, directory.path().join("missing.csljson")] {
        assert!(
            Current::open(
                ibis_engine::Engine::new().unwrap_or_else(|_| panic!("engine")),
                Source::Samples(path),
                None,
                &Cancellation::default()
            )
            .is_err(),
            "malformed or unreadable samples cannot become a loaded sample library"
        );
    }
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        config["bundle"]["resources"]
            .as_array()
            .unwrap_or_else(|| panic!("resources"))
            .contains(&json!("resources/samples.csljson"))
    );
    assert!(
        source().setting().is_none(),
        "samples never become a saved library choice"
    );
    let library = loaded(source());
    let status = library.status();
    assert!(status.sample);
    assert!(!status.zotero);
    assert_eq!(status.name.as_deref(), Some("Sample references"));
    assert_eq!(status.snapshot_name, status.name);
    assert_eq!((status.references, status.warnings), (3, 0));
    let file = library.current().unwrap_or_else(|| panic!("engine"));
    let query =
        serde_json::from_value(json!({"text":"lantern","interpret_locators":false})).unwrap_or_else(|e| panic!("{e}"));
    let rows = file
        .search(&query, 10, None, &Cancellation::default())
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(rows.results.len(), 1);
    assert!(rows.results[0].sample);
    assert_eq!(
        file.engine
            .libraries()
            .unwrap_or_else(|_| panic!("libraries"))
            .libraries
            .len(),
        1
    );
    for (term, text) in [
        ("lantern", "(Vale, 2024) [Sample]"),
        ("clockwork", "(Fen, 2023) [Sample]"),
        ("islands", "(Reed & Vale, 2025) [Sample]"),
    ] {
        let receipt = preview(&file, term);
        assert_eq!(receipt.text, text);
        let copied = crate::test_citations::copy(&file.engine, &receipt.ticket, &receipt.content_hash, |content| {
            assert_eq!(content.text, text);
            assert!(content.html.is_some() && content.rtf.is_some());
            for representation in [Some(content.text), content.html, content.rtf].into_iter().flatten() {
                assert!(representation.contains("[Sample]"));
            }
            Ok(())
        })
        .unwrap_or_else(|_| panic!("copy"));
        assert_eq!(copied.text, text);
        assert_eq!(copied.warnings, receipt.warnings);
    }
}
