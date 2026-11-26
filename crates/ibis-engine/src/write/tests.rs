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
        again.err().map(Problem::from),
        Some(WriteProblem::AlreadyInLibrary.problem())
    );
    assert_eq!(
        fixture
            .engine
            .apply_proposal(&fixture.store, &proposal.id)
            .err()
            .map(Problem::from),
        Some(WriteProblem::Decided.problem())
    );
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
    let refused = fixture.engine.apply_proposal(&fixture.store, &proposal.id);
    let problem = refused.unwrap_err();
    assert_eq!(problem.problem(), &WriteProblem::Changed.problem());
    assert_eq!(fixture.store.show(&proposal.id).unwrap().refusal, Some(problem.into()));
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
            refused.problem(),
            WriteError::about(WriteProblem::Invalid, name).problem()
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
    assert_eq!(
        refused.err().map(Problem::from),
        Some(WriteError::about(WriteProblem::Invalid, "a, b").into())
    );
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
    assert_eq!(refused.err().map(Problem::from), Some(WriteProblem::NotOpen.problem()));
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
    assert_eq!(refused.err().map(Problem::from), Some(WriteProblem::NotOpen.problem()));
    assert_eq!(fs::read_to_string(&fixture.bib).unwrap(), BIB);
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
            .problem(),
        &WriteProblem::BackupFailed.problem()
    );
    assert_eq!(fs::read_to_string(&fixture.bib).unwrap(), BIB);
    assert_eq!(fixture.store.get(&proposal.id).unwrap().status, Status::Pending);
    assert!(backups(&fixture.bib).is_empty());
}

/// Two files can come to be read as one library (a link retargeted and
/// reloaded). A proposal goes to the file whose own read allows it, never to
/// the link, whichever read was installed last.
#[cfg(unix)]
#[test]
fn a_library_read_from_two_files_takes_proposals_only_where_its_read_allows() {
    let fixture = fixture();
    let dir = fixture.bib.parent().unwrap();
    let other = dir.join("other.bib");
    fs::write(&other, BIB).unwrap();
    let link = dir.join("linked.json");
    std::os::unix::fs::symlink(&other, &link).unwrap();
    // The link is read first, so it would be the first file found for the library.
    let engine = Engine::new().unwrap().without_file_watches();
    engine.open_file(&link, &Cancellation::default()).unwrap();
    let linked = engine.libraries().unwrap().libraries[0].id.clone();
    engine.open_file(&fixture.bib, &Cancellation::default()).unwrap();
    let libraries = engine.libraries().unwrap().libraries;
    let library = libraries.into_iter().find(|info| info.id != linked).unwrap().id;
    fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(&fixture.bib, &link).unwrap();
    engine.reload_files(Some(&linked), &Cancellation::default()).unwrap();
    engine.reload_files(Some(&library), &Cancellation::default()).unwrap();
    let proposal = engine
        .propose(
            &fixture.store,
            source(),
            ProposeRequest::Add {
                library: Some(library),
                fields: paper(),
            },
        )
        .unwrap();
    assert_eq!(proposal.target.path, fixture.bib);
}
