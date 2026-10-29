//! The bundled data uses the production CSL-JSON reader and citation receipts.
use super::engine_file::FileSelection;
use super::load::{Loaded, load_source};
use super::state::{begin_open, install_loaded, leave_samples};
use super::status::status;
use super::*;
use ibis_core::cite::KeyFormat;
use ibis_core::settings::CitationStyle;
use ibis_engine::Cancellation;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Mutex;

fn source() -> Source {
    Source::Samples(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/samples.csljson"))
}

fn loaded(source: Source) -> Library {
    match load_source(source, false, None, &Cancellation::default(), || false).unwrap_or_else(|e| panic!("{e}")) {
        Loaded::New(library) => *library,
        Loaded::Unchanged => panic!("first read"),
    }
}

fn preview(file: &engine_file::FileLibrary, term: &str) -> engine_file::FilePreview {
    preview_output(file, term, KeyFormat::Pandoc, Some(CitationStyle::Apa))
}

fn preview_output(
    file: &engine_file::FileLibrary,
    term: &str,
    format: KeyFormat,
    style: Option<CitationStyle>,
) -> engine_file::FilePreview {
    let query =
        serde_json::from_value(json!({"text":term,"interpret_locators":false})).unwrap_or_else(|e| panic!("{e}"));
    let found = file
        .search(&query, 10, None, &Cancellation::default())
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(found.results.len(), 1);
    let row = &found.results[0];
    file.preview(
        file.reserve_preview(),
        vec![FileSelection {
            id: row.id.clone(),
            cite: row.cite.clone(),
            locator: None,
            form: None,
        }],
        format,
        style,
    )
    .unwrap_or_else(|_| panic!("preview"))
}

#[test]
fn bundled_samples_load_through_csl_json_and_copy_marked_apa() {
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
    let status = status(&library);
    assert!(status.sample);
    assert!(!status.zotero);
    assert_eq!(status.name.as_deref(), Some("Sample references"));
    assert_eq!(status.snapshot_name, status.name);
    assert_eq!((status.references, status.warnings), (3, 0));
    let file = library.file.unwrap_or_else(|| panic!("engine"));
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
        let copied = file
            .copy(&receipt.ticket, &receipt.content_hash, |content| {
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

#[test]
fn every_sample_style_and_key_output_keeps_its_marker_on_the_clipboard() {
    let file = loaded(source()).file.unwrap_or_else(|| panic!("engine"));
    for style in [
        CitationStyle::Apa,
        CitationStyle::ChicagoAuthorDate,
        CitationStyle::HarvardCiteThemRight,
        CitationStyle::Ieee,
        CitationStyle::ModernLanguageAssociation,
    ] {
        let receipt = preview_output(&file, "lantern", KeyFormat::Pandoc, Some(style));
        assert!(receipt.text.ends_with(" [Sample]"), "{style:?}");
        file.copy(&receipt.ticket, &receipt.content_hash, |content| {
            assert_eq!(content.text, receipt.text);
            assert!(content.html.is_some() && content.rtf.is_some(), "{style:?}");
            for representation in [Some(content.text), content.html, content.rtf].into_iter().flatten() {
                assert!(representation.contains("[Sample]"), "{style:?}");
            }
            Ok(())
        })
        .unwrap_or_else(|_| panic!("copy {style:?}"));
    }
    for (format, expected) in [
        (KeyFormat::Pandoc, "[@vale2024lanterns] [Sample]"),
        (KeyFormat::Autocite, "\\autocite{vale2024lanterns} [Sample]"),
        (KeyFormat::Parencite, "\\parencite{vale2024lanterns} [Sample]"),
        (KeyFormat::Textcite, "\\textcite{vale2024lanterns} [Sample]"),
        (KeyFormat::Cite, "\\cite{vale2024lanterns} [Sample]"),
        (KeyFormat::Typst, "@vale2024lanterns [Sample]"),
        (KeyFormat::Keys, "vale2024lanterns [Sample]"),
    ] {
        let receipt = preview_output(&file, "lantern", format, None);
        assert_eq!(receipt.text, expected, "{format:?}");
        file.copy(&receipt.ticket, &receipt.content_hash, |content| {
            assert_eq!(content.text, expected);
            for representation in [Some(content.text), content.html, content.rtf].into_iter().flatten() {
                assert!(representation.contains("[Sample]"), "{format:?}");
            }
            Ok(())
        })
        .unwrap_or_else(|_| panic!("copy {format:?}"));
    }
}

#[test]
fn sample_provenance_changes_the_prepared_hash_and_normal_library_copies_stay_unmarked() {
    let Source::Samples(path) = source() else {
        panic!("sample source")
    };
    let normal = engine_file::FileLibrary::open(&path).unwrap_or_else(|e| panic!("{e}"));
    let original = preview(&normal, "lantern");
    assert_eq!(original.text, "(Vale, 2024)");
    normal
        .copy(&original.ticket, &original.content_hash, |content| {
            for representation in [Some(content.text), content.html, content.rtf].into_iter().flatten() {
                assert!(!representation.contains("[Sample]"));
            }
            Ok(())
        })
        .unwrap_or_else(|_| panic!("normal copy"));
    let samples = loaded(source()).file.unwrap_or_else(|| panic!("sample engine"));
    let marked = preview(&samples, "lantern");
    assert_ne!(marked.content_hash, original.content_hash);
    assert!(
        samples
            .copy(&marked.ticket, &original.content_hash, |_| panic!("unmarked preview"))
            .is_err()
    );
}

#[test]
fn leaving_samples_clears_the_snapshot_and_retires_pending_loads_and_copy_receipts() {
    let mut samples = loaded(source());
    samples.settings_notice = Some("Settings recovered".to_string());
    let file = samples.file.clone().unwrap_or_else(|| panic!("engine"));
    let receipt = preview(&file, "lantern");
    let library = Mutex::new(samples);
    let requests = LoadRequests::default();
    leave_samples(&library, &requests);
    let status = status(&library.lock().unwrap_or_else(|e| panic!("{e}")));
    assert!(!status.sample);
    assert!(!status.has_snapshot);
    assert_eq!(status.state, LibraryState::None);
    assert_eq!(status.references, 0);
    assert_eq!(status.settings_notice.as_deref(), Some("Settings recovered"));
    assert!(
        file.copy(&receipt.ticket, &receipt.content_hash, |_| panic!("retired copy"))
            .is_err()
    );
    let (pending, _, _) = begin_open(&library, &requests, source()).unwrap_or_else(|| panic!("open"));
    leave_samples(&library, &requests);
    assert!(
        install_loaded(
            &library,
            &requests,
            pending,
            Ok(Loaded::New(Box::new(loaded(source()))))
        )
        .is_none()
    );
}

#[test]
fn choosing_a_real_library_replaces_samples_and_a_failed_choice_keeps_them_labelled() {
    let library = Mutex::new(loaded(source()));
    let requests = LoadRequests::default();
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let path = directory.path().join("invented.bib");
    let (request, _, _) = begin_open(&library, &requests, path.clone()).unwrap_or_else(|| panic!("open"));
    let failed =
        install_loaded(&library, &requests, request, Err(LoadError::Stopped)).unwrap_or_else(|| panic!("current"));
    assert!(failed.sample);
    assert_eq!(failed.snapshot_name.as_deref(), Some("Sample references"));
    assert_eq!(failed.references, 3);
    std::fs::write(&path, "@book{own, title={Invented real-library entry}}").unwrap_or_else(|e| panic!("{e}"));
    let (request, _, _) = begin_open(&library, &requests, path.clone()).unwrap_or_else(|| panic!("open"));
    let real = install_loaded(
        &library,
        &requests,
        request,
        Ok(Loaded::New(Box::new(loaded(Source::File(path))))),
    )
    .unwrap_or_else(|| panic!("current"));
    assert!(!real.sample);
    assert_eq!(real.references, 1);
    let before = real.revision;
    leave_samples(&library, &requests);
    let library = library.lock().unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        status(&library).revision,
        before,
        "leaving samples cannot clear a real library"
    );
    let file = library.file.as_ref().unwrap_or_else(|| panic!("engine"));
    let query =
        serde_json::from_value(json!({"text":"lantern","interpret_locators":false})).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        file.search(&query, 10, None, &Cancellation::default())
            .unwrap_or_else(|e| panic!("{e}"))
            .results
            .is_empty()
    );
}
