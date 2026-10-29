#![allow(clippy::unwrap_used)]
use ibis_core::contract::*;
use ibis_engine::{Cancellation, Engine, Method, request::*};
use serde_json::json;

#[test]
fn file_to_search_to_citation_uses_one_private_source_owner() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.bib");
    std::fs::write(
        &path,
        "@article{demo,title={Synthetic research},author={Researcher, Synthetic},year={2026}}\n",
    )
    .unwrap();
    let engine = Engine::new().unwrap();
    let cancel = Cancellation::default();
    let opened = engine
        .dispatch(Method::Libraries, &json!({"action":"open-file","path":path}), &cancel)
        .unwrap();
    assert_eq!(opened.exit_code, ExitCode::Ok);
    assert!(!opened.data.to_string().contains(dir.path().to_str().unwrap()));
    let hits = engine
        .search(
            &SearchRequest {
                interpret_locator: false,
                query: "research".into(),
                limit: 10,
                libraries: None,
            },
            &cancel,
        )
        .unwrap();
    assert_eq!(hits.hits.len(), 1);
    let result = engine
        .cite_keys(
            &KeyCitationRequest {
                keys: vec!["demo".into()],
                output: OutputSpec::Key {
                    syntax: KeySyntax::Pandoc,
                    command: None,
                },
                form: CitationForm::Normal,
                output_locale: None,
            },
            &cancel,
        )
        .unwrap();
    assert_eq!(result.copy_content(1).unwrap().text, "[@demo]");
    assert_eq!(engine.libraries().unwrap().libraries.len(), 1);
}

fn open(engine: &Engine, path: &std::path::Path) -> LibraryId {
    let result = engine
        .dispatch(
            Method::Libraries,
            &json!({"action":"open-file","path":path}),
            &Cancellation::default(),
        )
        .unwrap();
    assert_eq!(result.exit_code, ExitCode::Ok);
    engine.libraries().unwrap().libraries.last().unwrap().id.clone()
}
fn write_record(path: &std::path::Path, key: &str, title: &str) {
    std::fs::write(path, format!("@article{{{key},title={{{title}}},year={{2026}}}}\n")).unwrap();
}
fn wait_for(mut predicate: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !predicate() {
        assert!(
            std::time::Instant::now() < deadline,
            "synthetic source update timed out"
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}
#[test]
fn one_record_reload_reuses_unchanged_records_with_a_pending_citation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.bib");
    let write = |title: &str| {
        std::fs::write(
            &path,
            format!("@article{{a,title={{Unchanged synthetic}}}}\n@article{{b,title={{{title}}}}}\n"),
        )
        .unwrap();
    };
    write("Original synthetic");
    let engine = Engine::new().unwrap().without_file_watches();
    open(&engine, &path);
    let old = engine.snapshot().unwrap();
    let request: CitationRequest = serde_json::from_value(json!({
        "items": [{"item":old.objects()[0].object.id()}],
        "output": {"kind":"key","syntax":"pandoc","command":null},
        "keep_order": true,
    }))
    .unwrap();
    let prepared = engine.prepare_citation(&request, &Cancellation::default()).unwrap();
    write("Edited synthetic");
    engine
        .dispatch(Method::Libraries, &json!({"action":"reload"}), &Cancellation::default())
        .unwrap();
    let refreshed = engine.snapshot().unwrap();
    assert!(std::sync::Arc::ptr_eq(&old.objects()[0], &refreshed.objects()[0]));
    assert!(!std::sync::Arc::ptr_eq(&old.objects()[1], &refreshed.objects()[1]));
    let text = engine
        .commit_citation(&prepared, &prepared.result().content_hash, |content| {
            content.text.to_owned()
        })
        .unwrap();
    assert_eq!(text, "[@a]");
}
#[test]
fn damaged_refresh_keeps_last_good_data_and_exposes_fixed_status() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.bib");
    write_record(&path, "demo", "Original synthetic");
    let engine = Engine::new().unwrap();
    let id = open(&engine, &path);
    let old = engine.snapshot().unwrap();
    let updated = engine.libraries().unwrap().libraries[0].updated_at_ms;
    std::fs::write(&path, "@article{unfinished,title={").unwrap();
    let failure = engine
        .dispatch(
            Method::Libraries,
            &json!({"action":"reload","library":id}),
            &Cancellation::default(),
        )
        .unwrap_err();
    assert_eq!(failure.code, ErrorCode::BadInput);
    assert_eq!(engine.snapshot().unwrap().fingerprints(), old.fingerprints());
    let status = engine.libraries().unwrap();
    assert_eq!(status.libraries[0].refresh_error, Some(ErrorCode::BadInput));
    assert_eq!(status.libraries[0].updated_at_ms, updated);
    let listed = engine
        .dispatch(Method::Libraries, &json!({"action":"list"}), &Cancellation::default())
        .unwrap();
    assert_eq!(listed.exit_code, ExitCode::Findings);
    assert!(listed.source_findings);
    assert!(!listed.data.to_string().contains(dir.path().to_str().unwrap()));
    write_record(&path, "demo", "Repaired synthetic");
    engine
        .dispatch(
            Method::Libraries,
            &json!({"action":"reload","library":id}),
            &Cancellation::default(),
        )
        .unwrap();
    assert_eq!(engine.libraries().unwrap().libraries[0].refresh_error, None);
    assert_ne!(engine.snapshot().unwrap().fingerprints(), old.fingerprints());
    assert!(
        serde_json::to_string(old.as_ref())
            .unwrap()
            .contains("Original synthetic")
    );
}
#[test]
fn watcher_refreshes_search_and_notifies_without_mutating_prior_snapshots() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.json");
    let write = |title: &str| {
        std::fs::write(
            &path,
            serde_json::to_vec(
                &json!([{"id":"json-record","citation-key":"demo","type":"article-journal","title":title}]),
            )
            .unwrap(),
        )
        .unwrap()
    };
    write("Original synthetic");
    let engine = Engine::new().unwrap();
    open(&engine, &path);
    let old = engine.snapshot().unwrap();
    let subscription = engine.subscribe().unwrap();
    write("تجربة multilingual replacement");
    wait_for(|| engine.snapshot().unwrap().fingerprints() != old.fingerprints());
    let result = engine
        .search(
            &SearchRequest {
                interpret_locator: false,
                query: "تجربة".into(),
                limit: 10,
                libraries: None,
            },
            &Cancellation::default(),
        )
        .unwrap();
    assert_eq!(result.hits.len(), 1);
    assert!(
        serde_json::to_string(old.as_ref())
            .unwrap()
            .contains("Original synthetic")
    );
    subscription.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
    assert_eq!(&result.snapshot, engine.snapshot().unwrap().fingerprints());
    assert_eq!(engine.libraries().unwrap().libraries[0].refresh_error, None);
}
#[cfg(unix)]
#[test]
fn watched_symlink_retires_old_identity_but_keeps_other_selected_owners() {
    let dir = tempfile::tempdir().unwrap();
    let selected = dir.path().join("selected.bib");
    let paths: Vec<_> = ["first", "second", "third"]
        .map(|name| {
            let path = dir.path().join(format!("{name}.bib"));
            write_record(&path, name, name);
            path
        })
        .into();
    std::os::unix::fs::symlink(&paths[0], &selected).unwrap();
    let engine = Engine::new().unwrap();
    let first = open(&engine, &selected);
    open(&engine, &paths[1]);
    let second = ibis_sources::load_file(&paths[1]).unwrap().library;
    let third = ibis_sources::load_file(&paths[2]).unwrap().library;
    let retarget = |path: &std::path::Path| {
        std::fs::remove_file(&selected).unwrap();
        std::os::unix::fs::symlink(path, &selected).unwrap();
    };
    retarget(&paths[1]);
    wait_for(|| {
        let libraries = engine.libraries().unwrap().libraries;
        libraries.len() == 1 && libraries[0].id == second
    });
    retarget(&paths[2]);
    wait_for(|| {
        let ids: Vec<_> = engine
            .libraries()
            .unwrap()
            .libraries
            .into_iter()
            .map(|info| info.id)
            .collect();
        ids.len() == 2 && ids.contains(&second) && ids.contains(&third) && !ids.contains(&first)
    });
}
