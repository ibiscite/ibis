//! Explicitly downloaded files use the ordinary local-library contract.
#![allow(clippy::unwrap_used)]
use ibis_core::contract::*;
use ibis_engine::{
    Cancellation, Engine, KeyCheck, Method, ProjectAssessmentState, ProjectProblem, ProjectResult,
    request::{ItemSelector, KeyCitationRequest, SearchRequest, ShowRequest},
    settings::writings::Writing,
};
use serde_json::{Value, json};
use std::{fs, path::Path};

const MAPS: &str = "@article{Vale2026:Maps,title={Invented lunar maps},author={Vale, Ada and Moss, Eli},year={2026},doi={10.1234/invented.maps}}\n";
const ATLAS: &str = "@book{Chen2025-Atlas,title={Invented tidal atlas},author={Chen, Lin and Reed, Uma},date={2025-02-03},doi={10.1234/invented.atlas}}\n";

fn directory() -> tempfile::TempDir {
    tempfile::tempdir_in(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target")).unwrap()
}

fn dispatch(engine: &Engine, method: Method, params: Value) -> ibis_engine::DispatchResult {
    engine.dispatch(method, &params, &Cancellation::default()).unwrap()
}

fn open(engine: &Engine, path: &Path) {
    let result = dispatch(engine, Method::Libraries, json!({"action":"open-file","path":path}));
    assert_eq!(result.exit_code, ExitCode::Ok);
}

fn show(engine: &Engine, key: &str) -> Record {
    let shown = engine
        .show(&ShowRequest {
            item: ItemSelector::Key(key.into()),
        })
        .unwrap();
    let LibraryObject::Record(record) = shown.object else {
        panic!("expected a record")
    };
    assert_eq!(record.citation_key().unwrap(), Some(key));
    record
}

fn search(engine: &Engine, query: &str) -> Vec<ItemRef> {
    engine
        .search(
            &SearchRequest {
                query: query.into(),
                limit: 10,
                libraries: None,
                interpret_locator: false,
            },
            &Cancellation::default(),
        )
        .unwrap()
        .hits
        .into_iter()
        .map(|hit| hit.item.object.id().clone())
        .collect()
}

fn request(keys: &[&str], syntax: KeySyntax, strict: bool) -> KeyCitationRequest {
    serde_json::from_value(json!({
        "keys":keys.iter().map(|key| json!({"key":key})).collect::<Vec<_>>(),
        "output":{"kind":"key","syntax":syntax},"strict":strict
    }))
    .unwrap()
}

fn writing(path: &Path) -> Writing {
    serde_json::from_value(json!({
        "name":"Invented manuscript", "format":{"kind":"key","syntax":"latex"}, "bibliography":path
    }))
    .unwrap()
}

fn unchanged(path: &Path, bytes: &[u8], permissions: &fs::Permissions) {
    assert_eq!(fs::read(path).unwrap(), bytes);
    assert_eq!(&fs::metadata(path).unwrap().permissions(), permissions);
}

#[test]
fn two_selected_downloads_expose_metadata_and_emit_the_exact_keys() {
    let dir = directory();
    let nested = dir.path().join("extracted-project/references");
    fs::create_dir_all(&nested).unwrap();
    let paths = [dir.path().join("download.bib"), nested.join("refs.bib")];
    // A nearby file is not selected merely because it was extracted too.
    fs::write(nested.join("unselected.bib"), "@book{Unselected,title={Unselected}}\n").unwrap();
    let engine = Engine::new().unwrap().without_file_watches();
    let mut permissions = Vec::new();
    for (path, text) in paths.iter().zip([MAPS, ATLAS]) {
        fs::write(path, text).unwrap();
        permissions.push(fs::metadata(path).unwrap().permissions());
        open(&engine, path);
    }
    assert_eq!(engine.libraries().unwrap().libraries.len(), 2);
    assert_eq!(engine.snapshot().unwrap().objects().len(), 2);
    for (key, title, authors, date, doi) in [
        (
            "Vale2026:Maps",
            "Invented lunar maps",
            json!([{"family":"Vale","given":"Ada"},{"family":"Moss","given":"Eli"}]),
            json!([[2026]]),
            "10.1234/invented.maps",
        ),
        (
            "Chen2025-Atlas",
            "Invented tidal atlas",
            json!([{"family":"Chen","given":"Lin"},{"family":"Reed","given":"Uma"}]),
            json!([[2025, 2, 3]]),
            "10.1234/invented.atlas",
        ),
    ] {
        let record = show(&engine, key);
        assert_eq!(record.csl.get("title"), Some(&json!(title)));
        assert_eq!(record.csl.get("author"), Some(&authors));
        assert_eq!(record.csl.get("issued"), Some(&json!({"date-parts":date})));
        assert_eq!(record.csl.get("DOI"), Some(&json!(doi)));
        assert_eq!(search(&engine, title), [record.id]);
    }
    assert!(search(&engine, "Unselected").is_empty());
    for (syntax, expected) in [
        (KeySyntax::Latex, r"\cite{Vale2026:Maps,Chen2025-Atlas}"),
        (KeySyntax::Biblatex, r"\autocites{Vale2026:Maps}{Chen2025-Atlas}"),
        (KeySyntax::Natbib, r"\citep{Vale2026:Maps,Chen2025-Atlas}"),
    ] {
        let result = engine
            .cite_keys(
                &request(&["Vale2026:Maps", "Chen2025-Atlas"], syntax, false),
                &Cancellation::default(),
            )
            .unwrap();
        assert_eq!(result.copy_content(2).unwrap().text, expected);
        assert!(result.losses.is_empty());
    }
    let checked = engine
        .cite_writing_keys(
            &writing(&paths[1]),
            &request(&["Chen2025-Atlas"], KeySyntax::Latex, true),
            &Cancellation::default(),
        )
        .unwrap();
    assert_eq!(checked.citation.copy_content(1).unwrap().text, "\\cite{Chen2025-Atlas}");
    assert!(checked.bibliography.unwrap().settled());
    for ((path, text), permission) in paths.iter().zip([MAPS, ATLAS]).zip(&permissions) {
        unchanged(path, text.as_bytes(), permission);
    }
}

#[test]
fn a_key_shared_by_two_downloads_refuses_lookup_and_citation() {
    let dir = directory();
    let engine = Engine::new().unwrap().without_file_watches();
    for (name, text) in [
        ("first.bib", MAPS),
        (
            "second.bib",
            "@article{Vale2026:Maps,title={Invented other maps},doi={10.1234/invented.other}}\n",
        ),
    ] {
        let path = dir.path().join(name);
        fs::write(&path, text).unwrap();
        open(&engine, &path);
    }
    assert_eq!(search(&engine, "Invented").len(), 2);
    let expected = EngineError::because(ErrorCode::BadInput, ErrorReason::AmbiguousKey);
    assert_eq!(
        engine
            .show(&ShowRequest {
                item: ItemSelector::Key("Vale2026:Maps".into())
            })
            .unwrap_err(),
        expected
    );
    for strict in [false, true] {
        assert_eq!(
            engine
                .cite_keys(
                    &request(&["Vale2026:Maps"], KeySyntax::Latex, strict),
                    &Cancellation::default()
                )
                .unwrap_err(),
            expected
        );
    }
}

#[test]
fn a_project_binding_the_emitted_key_to_another_doi_blocks_copy_even_with_damage() {
    let dir = directory();
    let source = dir.path().join("download.bib");
    let project = dir.path().join("manuscript.bib");
    fs::write(&source, MAPS).unwrap();
    let engine = Engine::new().unwrap().without_file_watches();
    open(&engine, &source);
    let permission = fs::metadata(&source).unwrap().permissions();
    for damage in ["", "@book{broken,title={Never closed\n"] {
        let text =
            format!("@article{{Vale2026:Maps,title={{Invented other work}},doi={{10.1234/invented.other}}}}\n{damage}");
        fs::write(&project, &text).unwrap();
        let permission_project = fs::metadata(&project).unwrap().permissions();
        let ordinary = engine
            .cite_writing_keys(
                &writing(&project),
                &request(&["Vale2026:Maps"], KeySyntax::Latex, false),
                &Cancellation::default(),
            )
            .unwrap();
        let assessment = ordinary.assessment.unwrap();
        assert_eq!(assessment.state, ProjectAssessmentState::Conflict);
        assert_eq!(assessment.conflicts, ["Vale2026:Maps"]);
        assert!(!assessment.can_copy());
        assert!(ordinary.citation.text.is_empty());
        assert!(ordinary.citation.copy_content(1).is_none());
        assert_eq!(
            ordinary.citation.outcomes,
            [ItemOutcome::Refused {
                item: 0,
                error: CitationError::ConflictingKey
            }]
        );
        assert_eq!(
            engine
                .cite_writing_keys(
                    &writing(&project),
                    &request(&["Vale2026:Maps"], KeySyntax::Latex, true),
                    &Cancellation::default()
                )
                .unwrap_err(),
            EngineError::because(ErrorCode::BadInput, ErrorReason::StrictCitation)
        );
        unchanged(&project, text.as_bytes(), &permission_project);
    }
    unchanged(&source, MAPS.as_bytes(), &permission);
}

#[test]
fn incomplete_project_checks_advise_ordinary_citations_and_refuse_strict_ones() {
    let dir = directory();
    let source = dir.path().join("download.bib");
    fs::write(&source, MAPS).unwrap();
    let engine = Engine::new().unwrap().without_file_watches();
    open(&engine, &source);
    // Invalid UTF-8 is unreadable on every platform, including privileged test runners.
    for (name, bytes, problem, check, damaged) in [
        (
            "unavailable.bib",
            None,
            Some(ProjectProblem::Unavailable),
            KeyCheck::CouldNotCheck,
            vec![],
        ),
        (
            "unreadable.bib",
            Some(&b"\xff"[..]),
            Some(ProjectProblem::NotUtf8),
            KeyCheck::CouldNotCheck,
            vec![],
        ),
        (
            "malformed.bib",
            Some(&b"@book{broken,title={Never closed\n"[..]),
            None,
            KeyCheck::CouldNotCheck,
            vec![1],
        ),
        ("empty.bib", Some(&b""[..]), None, KeyCheck::Missing, vec![]),
    ] {
        let path = dir.path().join(name);
        if let Some(bytes) = bytes {
            fs::write(&path, bytes).unwrap();
        }
        let permission = bytes.map(|_| fs::metadata(&path).unwrap().permissions());
        let checked = dispatch(&engine, Method::Project, json!({"path":path,"keys":["Vale2026:Maps"]}));
        assert_eq!(checked.exit_code, ExitCode::Findings);
        let checked: ProjectResult = serde_json::from_value(checked.data).unwrap();
        assert_eq!(checked.problem, problem);
        assert_eq!(checked.keys[0].check, check);
        assert_eq!(checked.damaged_lines, damaged);
        assert!(!checked.settled());
        let ordinary = engine
            .cite_writing_keys(
                &writing(&path),
                &request(&["Vale2026:Maps"], KeySyntax::Latex, false),
                &Cancellation::default(),
            )
            .unwrap();
        assert_eq!(ordinary.bibliography.unwrap(), checked);
        assert!(ordinary.assessment.unwrap().can_copy());
        assert_eq!(ordinary.citation.copy_content(1).unwrap().text, "\\cite{Vale2026:Maps}");
        assert_eq!(
            engine
                .cite_writing_keys(
                    &writing(&path),
                    &request(&["Vale2026:Maps"], KeySyntax::Latex, true),
                    &Cancellation::default()
                )
                .unwrap_err(),
            EngineError::because(ErrorCode::BadInput, ErrorReason::StrictCitation)
        );
        if let (Some(bytes), Some(permission)) = (bytes, permission) {
            unchanged(&path, bytes, &permission);
        }
    }
    // A failed source open is an error, not an advisory project check.
    let missing = engine
        .dispatch(
            Method::Libraries,
            &json!({"action":"open-file","path":dir.path().join("missing-source.bib")}),
            &Cancellation::default(),
        )
        .unwrap_err();
    assert_eq!(
        missing,
        EngineError::because(ErrorCode::SourceUnavailable, ErrorReason::MissingFile)
    );
    assert_eq!(engine.libraries().unwrap().libraries.len(), 1);
    open(&engine, &dir.path().join("empty.bib"));
    let empty = engine
        .libraries()
        .unwrap()
        .libraries
        .into_iter()
        .find(|library| library.objects == 0)
        .unwrap();
    assert_eq!((empty.objects, empty.warnings_total, empty.refresh_error), (0, 0, None));
    assert!(
        engine
            .show(&ShowRequest {
                item: ItemSelector::Key("Vale2026:Maps".into())
            })
            .is_ok()
    );
}

#[test]
fn replacing_a_download_and_explicitly_reloading_preserves_the_earlier_snapshot() {
    let dir = directory();
    let selected = dir.path().join("download.bib");
    fs::write(&selected, MAPS).unwrap();
    let engine = Engine::new().unwrap().without_file_watches();
    open(&engine, &selected);
    let earlier = engine.snapshot().unwrap();
    let earlier_bytes = serde_json::to_vec(earlier.as_ref()).unwrap();
    let earlier_id = show(&engine, "Vale2026:Maps").id;
    let library = earlier_id.library.clone();
    let replacement = dir.path().join("repeat-download.bib");
    fs::write(&replacement, ATLAS).unwrap();
    // Replace the selected file, as a completed repeat download would.
    fs::remove_file(&selected).unwrap();
    fs::rename(&replacement, &selected).unwrap();
    let permission = fs::metadata(&selected).unwrap().permissions();
    assert_eq!(show(&engine, "Vale2026:Maps").id, earlier_id);
    let reloaded = dispatch(&engine, Method::Libraries, json!({"action":"reload","library":library}));
    assert_eq!(reloaded.exit_code, ExitCode::Ok);
    assert_eq!(engine.libraries().unwrap().libraries.len(), 1);
    let updated = show(&engine, "Chen2025-Atlas");
    assert_eq!(updated.id.library, library);
    assert_eq!(updated.csl.get("title"), Some(&json!("Invented tidal atlas")));
    assert_eq!(
        updated.csl.get("author"),
        Some(&json!([{"family":"Chen","given":"Lin"},{"family":"Reed","given":"Uma"}]))
    );
    assert_eq!(updated.csl.get("issued"), Some(&json!({"date-parts":[[2025,2,3]]})));
    assert_eq!(updated.csl.get("DOI"), Some(&json!("10.1234/invented.atlas")));
    assert_eq!(search(&engine, "tidal"), [updated.id]);
    assert!(search(&engine, "lunar").is_empty());
    assert_eq!(
        engine
            .show(&ShowRequest {
                item: ItemSelector::Key("Vale2026:Maps".into())
            })
            .unwrap_err(),
        EngineError::new(ErrorCode::NotFound)
    );
    assert_eq!(
        engine
            .cite_keys(
                &request(&["Chen2025-Atlas"], KeySyntax::Natbib, false),
                &Cancellation::default()
            )
            .unwrap()
            .copy_content(1)
            .unwrap()
            .text,
        "\\citep{Chen2025-Atlas}"
    );
    assert_ne!(engine.snapshot().unwrap().fingerprints(), earlier.fingerprints());
    assert_eq!(serde_json::to_vec(earlier.as_ref()).unwrap(), earlier_bytes);
    let LibraryObject::Record(held) = &earlier.get(&earlier_id).unwrap().object else {
        panic!("held record")
    };
    assert_eq!(held.citation_key().unwrap(), Some("Vale2026:Maps"));
    assert_eq!(held.csl.get("title"), Some(&json!("Invented lunar maps")));
    unchanged(&selected, ATLAS.as_bytes(), &permission);
}
