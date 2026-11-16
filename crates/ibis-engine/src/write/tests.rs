use super::*;
use serde_json::json;
use std::fs;
use std::path::Path;

const BIB: &str = "% An invented library\n@string{pub = {Synthetic Press}}\n\n@article{morgan2025,\n  title = {Synthetic noise},\n  author = {Morgan, Ada},\n  journal = pub,\n  year = {2025},\n  keywords = {noise, {Signal Processing}},\n}\n\n@book{taylor2026,\n  title = {Synthetic signals},\n  author = {Taylor, Sam},\n  year = {2026}\n}\n";

struct Fixture {
    _dir: tempfile::TempDir,
    bib: PathBuf,
    store: Proposals,
    engine: Engine,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("library")).unwrap();
    let bib = dir.path().join("library/refs.bib");
    fs::write(&bib, BIB).unwrap();
    let engine = Engine::new().unwrap().without_file_watches();
    engine.open_file(&bib, &Cancellation::default()).unwrap();
    // The path as the engine holds it: on macOS, through the /var link.
    let bib = engine
        .file_path(&engine.libraries().unwrap().libraries[0].id)
        .unwrap()
        .unwrap();
    let store = Proposals::new(dir.path().join("proposals"));
    Fixture {
        _dir: dir,
        bib,
        store,
        engine,
    }
}

fn source() -> Source {
    Source {
        via: Via::Mcp,
        client: "Invented Client".into(),
    }
}

fn object(value: Value) -> Map<String, Value> {
    let Value::Object(map) = value else { panic!("an object") };
    map
}

fn item(fixture: &Fixture, key: &str) -> ItemRef {
    let found = fixture
        .engine
        .show(&ShowRequest {
            item: ItemSelector::Key(key.into()),
        })
        .unwrap();
    found.object.id().clone()
}

fn paper() -> Map<String, Value> {
    object(json!({
        "type": "article-journal",
        "title": "Quiet signals",
        "author": [{"family": "Çelik", "given": "Defne"}],
        "issued": {"date-parts": [[2024, 5]]},
        "container-title": "Invented Letters",
        "DOI": "10.5555/invented.1"
    }))
}

fn backups(path: &Path) -> Vec<PathBuf> {
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    let mut found: Vec<PathBuf> = fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|other| {
            other
                .file_name()
                .is_some_and(|other| other.to_string_lossy().starts_with(&format!("{name}.ibis-backup-")))
        })
        .collect();
    found.sort();
    found
}

#[test]
fn an_added_reference_is_appended_with_a_new_key_and_every_other_byte_kept() {
    let fixture = fixture();
    let proposal = fixture
        .engine
        .propose(
            &fixture.store,
            source(),
            ProposeRequest::Add {
                library: None,
                fields: paper(),
            },
        )
        .unwrap();
    assert_eq!(proposal.status, Status::Pending);
    let Change::Add { key, .. } = &proposal.change else {
        panic!("an addition")
    };
    assert_eq!(key, "celik2024quiet");
    // Proposing writes nothing to the library.
    assert_eq!(fs::read_to_string(&fixture.bib).unwrap(), BIB);
    assert!(backups(&fixture.bib).is_empty());

    let preview = fixture.store.show(&proposal.id).unwrap();
    assert_eq!(preview.refusal, None);
    assert_eq!(preview.before, None);
    assert_eq!(preview.library, "refs.bib");
    let entry = preview.after.clone().unwrap();
    assert!(entry.starts_with("@article{celik2024quiet,\n"), "{entry}");

    let applied = fixture.engine.apply_proposal(&fixture.store, &proposal.id).unwrap();
    assert_eq!(applied.proposal.status, Status::Applied);
    let written = fs::read_to_string(&fixture.bib).unwrap();
    assert_eq!(written, format!("{BIB}\n{entry}\n"));
    assert_eq!(fs::read_to_string(&applied.backup).unwrap(), BIB);
    assert!(backups(&fixture.bib).is_empty());
    assert!(
        applied
            .backup
            .starts_with(fixture.store.dir().with_file_name("library-backups"))
    );
    // The engine reads the change at once, and the reference is not offered twice.
    assert!(
        fixture
            .engine
            .show(&ShowRequest {
                item: ItemSelector::Key("celik2024quiet".into())
            })
            .is_ok()
    );
    let again = fixture.engine.propose(
        &fixture.store,
        source(),
        ProposeRequest::Add {
            library: None,
            fields: paper(),
        },
    );
    assert_eq!(
        again.err().map(|error| error.problem),
        Some(WriteProblem::AlreadyInLibrary)
    );
    assert_eq!(
        fixture
            .engine
            .apply_proposal(&fixture.store, &proposal.id)
            .err()
            .map(|error| error.problem),
        Some(WriteProblem::Decided)
    );
}

#[test]
fn two_pending_additions_never_share_a_key() {
    let fixture = fixture();
    let mut keys = vec![];
    for _ in 0..2 {
        let mut fields = paper();
        fields.remove("DOI");
        let proposal = fixture
            .engine
            .propose(&fixture.store, source(), ProposeRequest::Add { library: None, fields })
            .unwrap();
        let Change::Add { key, .. } = proposal.change else {
            panic!("an addition")
        };
        keys.push(key);
    }
    assert_eq!(keys, ["celik2024quiet", "celik2024quieta"]);
}

#[test]
fn a_field_change_edits_only_that_entry_in_the_names_it_uses() {
    let fixture = fixture();
    let fields =
        object(json!({"container-title": "Invented Review", "page": "10-20", "issued": {"date-parts": [[2025, 4]]}}));
    let proposal = fixture
        .engine
        .propose(
            &fixture.store,
            source(),
            ProposeRequest::Fields {
                item: item(&fixture, "morgan2025"),
                fields,
            },
        )
        .unwrap();
    let preview = fixture.store.show(&proposal.id).unwrap();
    assert_eq!(preview.refusal, None);
    let after = preview.after.unwrap();
    assert!(after.contains("journal = {Invented Review}"), "{after}");
    assert!(
        after.contains("date = {2025-04}") && !after.contains("year ="),
        "{after}"
    );
    fixture.engine.apply_proposal(&fixture.store, &proposal.id).unwrap();
    let written = fs::read_to_string(&fixture.bib).unwrap();
    assert_eq!(written, BIB.replace(&preview.before.unwrap(), &after));
}

#[test]
fn tags_keep_the_spelling_already_written() {
    let fixture = fixture();
    let proposal = fixture
        .engine
        .propose(
            &fixture.store,
            source(),
            ProposeRequest::Tags {
                item: item(&fixture, "morgan2025"),
                add: vec!["to read".into(), "Noise".into()],
                remove: vec!["signal processing".into()],
            },
        )
        .unwrap();
    fixture.engine.apply_proposal(&fixture.store, &proposal.id).unwrap();
    let written = fs::read_to_string(&fixture.bib).unwrap();
    assert_eq!(
        written,
        BIB.replace("keywords = {noise, {Signal Processing}}", "keywords = {noise, to read}")
    );
}

#[test]
fn a_reference_changed_since_the_proposal_is_not_written() {
    let fixture = fixture();
    let proposal = fixture
        .engine
        .propose(
            &fixture.store,
            source(),
            ProposeRequest::Fields {
                item: item(&fixture, "taylor2026"),
                fields: object(json!({"publisher": "Invented House"})),
            },
        )
        .unwrap();
    let edited = BIB.replace("Synthetic signals", "Synthetic signals, revised");
    fs::write(&fixture.bib, &edited).unwrap();
    assert_eq!(
        fixture.store.show(&proposal.id).unwrap().refusal,
        Some(WriteProblem::Changed)
    );
    let refused = fixture.engine.apply_proposal(&fixture.store, &proposal.id);
    assert_eq!(refused.err().map(|error| error.problem), Some(WriteProblem::Changed));
    assert_eq!(fs::read_to_string(&fixture.bib).unwrap(), edited);
    assert!(backups(&fixture.bib).is_empty());
    assert_eq!(fixture.store.get(&proposal.id).unwrap().status, Status::Pending);
    // A change to another entry does not stand in the way.
    let other = fixture
        .engine
        .propose(
            &fixture.store,
            source(),
            ProposeRequest::Tags {
                item: item(&fixture, "morgan2025"),
                add: vec!["later".into()],
                remove: vec![],
            },
        )
        .unwrap();
    fs::write(&fixture.bib, BIB).unwrap();
    assert!(fixture.engine.apply_proposal(&fixture.store, &other.id).is_ok());
}

#[test]
fn private_fields_and_unknown_records_are_refused_by_name() {
    let fixture = fixture();
    for (name, value) in [
        ("note", json!("private")),
        ("file", json!("/invented/paper.pdf")),
        ("id", json!("x")),
        ("URL", json!("file:///invented/paper.pdf")),
    ] {
        let mut fields = paper();
        fields.insert(name.into(), value);
        let refused = fixture
            .engine
            .propose(&fixture.store, source(), ProposeRequest::Add { library: None, fields })
            .unwrap_err();
        assert_eq!(
            (refused.problem, refused.field.as_deref()),
            (WriteProblem::Invalid, Some(name))
        );
    }
    let refused = fixture.engine.propose(
        &fixture.store,
        source(),
        ProposeRequest::Tags {
            item: item(&fixture, "morgan2025"),
            add: vec!["a, b".into()],
            remove: vec![],
        },
    );
    assert_eq!(refused.err().map(|error| error.problem), Some(WriteProblem::Invalid));
    assert!(fixture.store.list().unwrap().is_empty());
}

fn propose_add(fixture: &Fixture, title: &str) -> Proposal {
    let mut fields = paper();
    fields.insert("title".into(), json!(title));
    fields.remove("DOI");
    fixture
        .engine
        .propose(&fixture.store, source(), ProposeRequest::Add { library: None, fields })
        .unwrap()
}

#[test]
fn applies_at_the_same_moment_keep_both_changes() {
    let fixture = fixture();
    let first = propose_add(&fixture, "Quiet signals");
    let second = propose_add(&fixture, "Loud signals");
    std::thread::scope(|scope| {
        for proposal in [&first, &second] {
            let fixture = &fixture;
            scope.spawn(move || fixture.engine.apply_proposal(&fixture.store, &proposal.id).unwrap());
        }
    });
    let text = fs::read_to_string(&fixture.bib).unwrap();
    assert!(text.contains("{Quiet signals}") && text.contains("{Loud signals}"));
}

#[cfg(unix)]
#[test]
fn a_library_folder_replaced_by_a_link_is_not_written() {
    let fixture = fixture();
    let proposal = propose_add(&fixture, "Quiet signals");
    let folder = fixture.bib.parent().unwrap().to_owned();
    let moved = folder.with_file_name("moved");
    fs::rename(&folder, &moved).unwrap();
    std::os::unix::fs::symlink(&moved, &folder).unwrap();
    let refused = fixture.engine.apply_proposal(&fixture.store, &proposal.id);
    assert_eq!(refused.err().map(|error| error.problem), Some(WriteProblem::NotOpen));
    assert_eq!(fs::read_to_string(moved.join("refs.bib")).unwrap(), BIB);
}

#[test]
fn only_a_library_open_in_this_engine_is_written() {
    let fixture = fixture();
    let proposal = fixture
        .engine
        .propose(
            &fixture.store,
            source(),
            ProposeRequest::Add {
                library: None,
                fields: paper(),
            },
        )
        .unwrap();
    let other = Engine::new().unwrap().without_file_watches();
    let refused = other.apply_proposal(&fixture.store, &proposal.id);
    assert_eq!(refused.err().map(|error| error.problem), Some(WriteProblem::NotOpen));
    assert_eq!(fs::read_to_string(&fixture.bib).unwrap(), BIB);
}

#[test]
fn rejecting_changes_nothing_but_the_proposal() {
    let fixture = fixture();
    let proposal = fixture
        .engine
        .propose(
            &fixture.store,
            source(),
            ProposeRequest::Add {
                library: None,
                fields: paper(),
            },
        )
        .unwrap();
    let rejected = fixture.store.reject(&proposal.id).unwrap();
    assert_eq!(rejected.status, Status::Rejected);
    assert!(rejected.decided_ms.is_some());
    assert_eq!(
        fixture.store.reject(&proposal.id).err().map(|e| e.problem),
        Some(WriteProblem::Decided)
    );
    let refused = fixture.engine.apply_proposal(&fixture.store, &proposal.id);
    assert_eq!(refused.err().map(|error| error.problem), Some(WriteProblem::Decided));
    assert_eq!(fs::read_to_string(&fixture.bib).unwrap(), BIB);
    for id in ["../settings", "ABCDEF0123456789", "0123"] {
        assert_eq!(
            fixture.store.get(id).err().map(|error| error.problem),
            Some(WriteProblem::UnknownProposal)
        );
    }
}

#[test]
fn proposals_are_bounded_in_number_and_size() {
    let fixture = fixture();
    let library = fixture.engine.libraries().unwrap().libraries[0].id.clone();
    let target = Target {
        library,
        path: fixture.bib.clone(),
        resolved: fs::canonicalize(&fixture.bib).unwrap(),
    };
    let add = |key: &str| Change::Add {
        key: key.into(),
        fields: paper(),
    };
    for number in 0..MAX_PENDING {
        fixture
            .store
            .create(source(), target.clone(), add(&format!("k{number}")), None)
            .unwrap();
    }
    let refused = fixture.store.create(source(), target.clone(), add("over"), None);
    assert_eq!(refused.err().map(|error| error.problem), Some(WriteProblem::TooMany));
    let first = fixture.store.list().unwrap().pop().unwrap();
    fixture.store.reject(&first.id).unwrap();
    let mut fields = paper();
    fields.insert("abstract".into(), json!("x".repeat(MAX_PROPOSAL_BYTES)));
    let large = fixture.store.create(
        source(),
        target.clone(),
        Change::Add {
            key: "large".into(),
            fields,
        },
        None,
    );
    assert_eq!(large.err().map(|error| error.problem), Some(WriteProblem::TooLarge));
    assert!(fixture.store.create(source(), target, add("fits"), None).is_ok());
    assert_eq!(fixture.store.list().unwrap().len(), MAX_PENDING + 1);
}

#[cfg(unix)]
#[test]
fn proposal_files_are_private_to_the_person() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = fixture();
    let proposal = fixture
        .engine
        .propose(
            &fixture.store,
            source(),
            ProposeRequest::Add {
                library: None,
                fields: paper(),
            },
        )
        .unwrap();
    let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(fixture.store.dir()), 0o700);
    let file = fixture.store.dir().join(format!("{}.json", proposal.id));
    assert_eq!(mode(&file), 0o600);
    fixture.store.reject(&proposal.id).unwrap();
    assert_eq!(mode(&file), 0o600);
}

fn call(engine: &Engine, session: &mut crate::mcp::Session, name: &str, arguments: Value) -> Value {
    crate::mcp::handle(
        engine,
        session,
        &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": name, "arguments": arguments}}),
    )
    .unwrap()
}

fn initialized(session: &mut crate::mcp::Session, engine: &Engine) {
    crate::mcp::handle(
        engine,
        session,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": {"name": "Invented Client", "version": "1.0"}
        }}),
    )
    .unwrap();
}

fn listed(engine: &Engine, session: &mut crate::mcp::Session) -> Vec<Value> {
    let reply = crate::mcp::handle(
        engine,
        session,
        &json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list", "params": {}}),
    )
    .unwrap();
    reply["result"]["tools"].as_array().unwrap().clone()
}

#[test]
fn proposal_tools_are_offered_only_with_allow_writes() {
    let fixture = fixture();
    let mut session = crate::mcp::Session::default();
    initialized(&mut session, &fixture.engine);
    let names = |tools: &[Value]| {
        tools
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    assert!(
        !names(&listed(&fixture.engine, &mut session))
            .iter()
            .any(|name| name.starts_with("propose"))
    );
    let refused = call(&fixture.engine, &mut session, "propose_add", json!({"fields": paper()}));
    assert_eq!(refused["error"]["message"], "Unknown tool");
    assert!(fixture.store.list().unwrap().is_empty());

    let mut session = crate::mcp::Session::default().allowing_writes(fixture.store.clone());
    initialized(&mut session, &fixture.engine);
    let tools = listed(&fixture.engine, &mut session);
    for name in ["propose_add", "propose_change", "propose_tags", "proposals"] {
        assert!(names(&tools).contains(&name.to_owned()), "{name}");
    }
    assert!(!names(&tools).iter().any(|name| name.contains("apply")));
    let add = tools.iter().find(|tool| tool["name"] == "propose_add").unwrap();
    assert_eq!(add["annotations"]["readOnlyHint"], false);
    assert_eq!(add["annotations"]["destructiveHint"], false);

    let answer = call(&fixture.engine, &mut session, "propose_add", json!({"fields": paper()}));
    let content = &answer["result"]["structuredContent"];
    assert_eq!(answer["result"]["isError"], false, "{answer}");
    assert_eq!(content["status"], "pending");
    let id = content["proposal"].as_str().unwrap();
    assert!(
        content["bibtex"]
            .as_str()
            .unwrap()
            .starts_with("@article{celik2024quiet,")
    );
    assert!(answer["result"]["content"][0]["text"].as_str().unwrap().contains(id));
    assert_eq!(fs::read_to_string(&fixture.bib).unwrap(), BIB);
    assert_eq!(fixture.store.get(id).unwrap().source.client, "Invented Client");

    let mine = call(&fixture.engine, &mut session, "proposals", json!({}));
    assert_eq!(mine["result"]["structuredContent"]["proposals"][0]["proposal"], id);
    assert_eq!(mine["result"]["structuredContent"]["libraries"][0]["kind"], "bibtex");
    let refused = call(
        &fixture.engine,
        &mut session,
        "propose_add",
        json!({"fields": {"type": "book", "title": "A", "note": "private"}}),
    );
    assert_eq!(refused["result"]["isError"], true);
    assert!(
        refused["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("note")
    );
}

#[test]
fn applies_keep_only_the_latest_five_private_backups_per_library() {
    let fixture = fixture();
    let other = fixture._dir.path().join("other/refs.bib");
    fs::create_dir(other.parent().unwrap()).unwrap();
    fs::write(&other, BIB).unwrap();
    fixture.engine.open_file(&other, &Cancellation::default()).unwrap();
    let mut directories = Vec::new();
    for library in fixture.engine.libraries().unwrap().libraries {
        let path = fixture.engine.file_path(&library.id).unwrap().unwrap();
        let mut versions = Vec::new();
        for i in 0..12 {
            let before = fs::read_to_string(&path).unwrap();
            let proposal = fixture
                .engine
                .propose(
                    &fixture.store,
                    source(),
                    ProposeRequest::Add {
                        library: Some(library.id.clone()),
                        fields: object(json!({"type":"book", "title":format!("Invented version {i}")})),
                    },
                )
                .unwrap();
            let applied = fixture.engine.apply_proposal(&fixture.store, &proposal.id).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    fs::metadata(&applied.backup).unwrap().permissions().mode() & 0o777,
                    0o600
                );
                assert_eq!(
                    fs::metadata(applied.backup.parent().unwrap())
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    0o700
                );
            }
            versions.push((applied.backup, before));
        }
        let directory = versions.last().unwrap().0.parent().unwrap().to_path_buf();
        assert_eq!(fs::read_dir(&directory).unwrap().count(), MAX_BACKUPS);
        for (i, (backup, before)) in versions.iter().enumerate() {
            if i < versions.len() - MAX_BACKUPS {
                assert!(!backup.exists());
            } else {
                assert_eq!(fs::read_to_string(backup).unwrap(), *before);
            }
        }
        assert!(backups(&path).is_empty());
        directories.push(directory);
    }
    assert_ne!(
        directories[0], directories[1],
        "same filenames in different libraries stay separate"
    );
    assert_eq!(fs::read_dir(&directories[0]).unwrap().count(), MAX_BACKUPS);
}

#[test]
fn rejecting_an_app_covers_all_its_libraries_and_preserves_other_apps_and_decisions() {
    let fixture = fixture();
    let mut proposals = Vec::new();
    for i in 0..4 {
        proposals.push(
            fixture
                .engine
                .propose(
                    &fixture.store,
                    Source {
                        client: if i == 3 { "Other app".into() } else { source().client },
                        ..source()
                    },
                    ProposeRequest::Add {
                        library: None,
                        fields: object(json!({"type":"book", "title":format!("Invented proposal {i}")})),
                    },
                )
                .unwrap(),
        );
    }
    // A pending proposal in another library from the same app.
    let other = fixture._dir.path().join("other.bib");
    fs::write(&other, BIB).unwrap();
    fixture.engine.open_file(&other, &Cancellation::default()).unwrap();
    let library = fixture
        .engine
        .libraries()
        .unwrap()
        .libraries
        .into_iter()
        .find(|info| fixture.engine.file_path(&info.id).unwrap().as_ref() != Some(&fixture.bib))
        .unwrap();
    proposals.push(
        fixture
            .engine
            .propose(
                &fixture.store,
                source(),
                ProposeRequest::Add {
                    library: Some(library.id),
                    fields: object(json!({"type":"book", "title":"Other library proposal"})),
                },
            )
            .unwrap(),
    );
    fixture.engine.apply_proposal(&fixture.store, &proposals[0].id).unwrap();
    fixture.store.reject(&proposals[1].id).unwrap();
    let before = fs::read_to_string(&fixture.bib).unwrap();
    assert_eq!(fixture.store.reject_from(&source()).unwrap(), 2);
    assert_eq!(fixture.store.reject_from(&source()).unwrap(), 0);
    for (proposal, status) in proposals.iter().zip([
        Status::Applied,
        Status::Rejected,
        Status::Rejected,
        Status::Pending,
        Status::Rejected,
    ]) {
        assert_eq!(fixture.store.get(&proposal.id).unwrap().status, status);
    }
    assert_eq!(fs::read_to_string(&fixture.bib).unwrap(), before);
    assert_eq!(fs::read_to_string(other).unwrap(), BIB);
}

#[test]
fn waiting_reads_each_library_once_including_failures_and_skips_decided_proposals() {
    let fixture = fixture();
    let mut proposals = Vec::new();
    for _ in 0..3 {
        let mut fields = paper();
        fields.remove("DOI");
        proposals.push(
            fixture
                .engine
                .propose(&fixture.store, source(), ProposeRequest::Add { library: None, fields })
                .unwrap(),
        );
    }
    fixture.store.reject(&proposals[0].id).unwrap();
    bib::READS.with_borrow_mut(Vec::clear);
    let waiting = fixture.store.waiting().unwrap();
    assert_eq!(waiting.len(), 2);
    assert!(
        waiting
            .iter()
            .all(|preview| preview.refusal.is_none() && preview.after.is_some())
    );
    bib::READS.with_borrow(|reads| assert_eq!(reads, std::slice::from_ref(&fixture.bib)));
    assert_eq!(fs::read_to_string(&fixture.bib).unwrap(), BIB);
    fs::remove_file(&fixture.bib).unwrap();
    bib::READS.with_borrow_mut(Vec::clear);
    let waiting = fixture.store.waiting().unwrap();
    assert!(
        waiting
            .iter()
            .all(|preview| preview.refusal == Some(WriteProblem::Unreadable))
    );
    bib::READS.with_borrow(|reads| assert_eq!(reads, std::slice::from_ref(&fixture.bib)));
}

#[test]
fn a_failed_backup_leaves_the_library_and_proposal_unchanged() {
    let fixture = fixture();
    let proposal = fixture
        .engine
        .propose(
            &fixture.store,
            source(),
            ProposeRequest::Add {
                library: None,
                fields: paper(),
            },
        )
        .unwrap();
    fs::write(fixture.store.dir().with_file_name("library-backups"), "blocked").unwrap();
    assert_eq!(
        fixture
            .engine
            .apply_proposal(&fixture.store, &proposal.id)
            .unwrap_err()
            .problem,
        WriteProblem::BackupFailed
    );
    assert_eq!(fs::read_to_string(&fixture.bib).unwrap(), BIB);
    assert_eq!(fixture.store.get(&proposal.id).unwrap().status, Status::Pending);
    assert!(backups(&fixture.bib).is_empty());
}

#[test]
fn replacement_rechecks_the_library_and_removes_the_backup_on_a_raced_edit() {
    let fixture = fixture();
    let proposal = fixture
        .engine
        .propose(
            &fixture.store,
            source(),
            ProposeRequest::Add {
                library: None,
                fields: paper(),
            },
        )
        .unwrap();
    let result = fixture
        .store
        .replace_library(&proposal.target, BIB.as_bytes(), b"proposed replacement", || {
            fs::write(&fixture.bib, b"external edit").unwrap();
            Err(WriteProblem::Changed)
        });
    assert_eq!(result.unwrap_err().problem, WriteProblem::Changed);
    assert_eq!(fs::read(&fixture.bib).unwrap(), b"external edit");
    let root = fixture.store.dir().with_file_name("library-backups");
    let library = fs::read_dir(root).unwrap().next().unwrap().unwrap().path();
    assert_eq!(fs::read_dir(library).unwrap().count(), 0);
    assert_eq!(fs::read_dir(fixture.bib.parent().unwrap()).unwrap().count(), 1);
}
