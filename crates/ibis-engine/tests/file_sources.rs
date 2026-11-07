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
    assert_eq!(engine.recite(&prepared, &Cancellation::default()).unwrap().text, "[@a]");
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
    // Not UTF-8: the refresh fails as an open would, so the old data stays.
    std::fs::write(&path, b"@article{demo,title={\xff}}").unwrap();
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
/// The stress-test regression: a library opened with one damaged entry never
/// refreshed again, so its edited titles were never found.
#[test]
fn a_library_opened_with_one_damaged_entry_reloads_its_edits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.bib");
    let library = |title: &str| {
        format!("@book{{first,title={{{title}}}}}\n@book{{broken,title={{Unclosed\n@book{{second,title={{Other}}}}\n")
    };
    std::fs::write(&path, library("Synthetic lantern")).unwrap();
    let engine = Engine::new().unwrap();
    let cancel = Cancellation::default();
    let opened = engine
        .dispatch(Method::Libraries, &json!({"action":"open-file","path":path}), &cancel)
        .unwrap();
    let id = opened.data["libraries"][0]["id"].as_str().unwrap().to_owned();
    let status = || engine.libraries().unwrap().libraries[0].clone();
    assert_eq!((status().objects, status().warnings_total), (2, 1));
    let hits = |query: &str| {
        engine
            .search(
                &SearchRequest {
                    interpret_locator: false,
                    query: query.into(),
                    limit: 10,
                    libraries: None,
                },
                &cancel,
            )
            .unwrap()
            .hits
            .len()
    };
    let reload = || {
        engine
            .dispatch(Method::Libraries, &json!({"action":"reload","library":id}), &cancel)
            .unwrap()
    };
    std::fs::write(&path, library("Synthetic zebra")).unwrap();
    reload();
    assert_eq!(status().refresh_error, None);
    assert_eq!((status().objects, status().warnings_total), (2, 1));
    assert_eq!((hits("zebra"), hits("lantern")), (1, 0));
    // The exact bytes it opened with reload too.
    std::fs::write(&path, library("Synthetic lantern")).unwrap();
    reload();
    assert_eq!(status().refresh_error, None);
    assert_eq!((hits("zebra"), hits("lantern")), (0, 1));
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

#[test]
fn a_repeated_key_is_named_and_selects_nothing_until_it_is_fixed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.bib");
    let engine = Engine::new().unwrap().without_file_watches();
    let cancel = Cancellation::default();
    let show = |key: &str| {
        engine.show(&ShowRequest {
            item: ItemSelector::Key(key.into()),
        })
    };
    let cite = |key: &str| {
        engine.cite_keys(
            &KeyCitationRequest {
                keys: vec![key.into()],
                output: OutputSpec::Key {
                    syntax: KeySyntax::Pandoc,
                    command: None,
                },
                form: CitationForm::Normal,
                output_locale: None,
            },
            &cancel,
        )
    };
    let repeated = EngineError::because(ErrorCode::NotFound, ErrorReason::DuplicateKey);
    for copies in [2, 3] {
        let mut text = "@article{kept,title={Kept synthetic}}\n".to_owned();
        for copy in 0..copies {
            text.push_str(&format!("@article{{twin,title={{Synthetic copy {copy}}}}}\n"));
        }
        std::fs::write(&path, text).unwrap();
        let action = if copies == 2 {
            json!({"action":"open-file","path":path})
        } else {
            json!({"action":"reload"})
        };
        let loaded = engine.dispatch(Method::Libraries, &action, &cancel).unwrap();
        assert_eq!(loaded.exit_code, ExitCode::Findings);
        let warnings = &engine.libraries().unwrap().libraries[0].warnings;
        let lines: Vec<_> = warnings
            .iter()
            .filter(|warning| warning.key.as_deref() == Some("twin"))
            .filter_map(|warning| warning.line)
            .collect();
        // Every entry under the key is named, the first included.
        assert_eq!(lines, (2..2 + copies).collect::<Vec<_>>(), "{copies} copies");
        assert_eq!(show("twin").unwrap_err(), repeated);
        assert_eq!(cite("twin").unwrap_err(), repeated);
        assert!(show("kept").is_ok());
        // A key nobody wrote is still simply missing.
        assert_eq!(show("absent").unwrap_err(), EngineError::new(ErrorCode::NotFound));
    }
    std::fs::write(&path, "@article{twin,title={Synthetic copy}}\n").unwrap();
    engine
        .dispatch(Method::Libraries, &json!({"action":"reload"}), &cancel)
        .unwrap();
    assert!(engine.libraries().unwrap().libraries[0].warnings.is_empty());
    assert!(show("twin").is_ok());
    assert_eq!(cite("twin").unwrap().copy_content(1).unwrap().text, "[@twin]");
}

#[test]
fn a_repeated_csl_json_id_refuses_every_record_and_lookup_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.json");
    std::fs::write(
        &path,
        r#"[{"id":"x","citation-key":"first","type":"book"},{"id":"x","citation-key":"second","type":"book"}]"#,
    )
    .unwrap();
    let engine = Engine::new().unwrap().without_file_watches();
    engine
        .dispatch(
            Method::Libraries,
            &json!({"action":"open-file","path":path}),
            &Cancellation::default(),
        )
        .unwrap();
    let warnings = &engine.libraries().unwrap().libraries[0].warnings;
    assert_eq!(warnings.len(), 2);
    assert!(warnings.iter().all(|warning| warning.key.as_deref() == Some("x")));
    let show = |key: &str| {
        engine.show(&ShowRequest {
            item: ItemSelector::Key(key.into()),
        })
    };
    assert_eq!(show("first").unwrap_err(), EngineError::new(ErrorCode::NotFound));
    assert_eq!(show("second").unwrap_err(), EngineError::new(ErrorCode::NotFound));
    assert_eq!(
        show("x").unwrap_err(),
        EngineError::because(ErrorCode::NotFound, ErrorReason::DuplicateKey)
    );
}
