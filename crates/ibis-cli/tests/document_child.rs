//! The engine reads a manuscript through the real `ibis __document` child
//! (decision 0060), never in its own process.
use ibis_engine::{Cancellation, CheckRequest, CheckResult, Engine};
use ibis_sources::document::{CHILD_ARGUMENT, Refusal};
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn the_engine_checks_a_manuscript_through_the_child() {
    let folder = tempfile::tempdir().unwrap();
    // macOS reaches its temporary folder through a link; documents are read without following one.
    let root = folder.path().canonicalize().unwrap();
    let library = root.join("refs.bib");
    std::fs::write(
        &library,
        "@book{vale2024, author = {Vale, Avery}, title = {Lanterns of the North}, year = {2024}}\n",
    )
    .unwrap();
    let draft = root.join("draft.tex");
    std::fs::write(&draft, "As \\textcite{vale2024} and \\cite{ghost} show.\n").unwrap();
    let engine = Engine::new()
        .unwrap()
        .without_file_watches()
        .with_document_reader(env!("CARGO_BIN_EXE_ibis").into());
    engine.open_file_observed(&library, &Cancellation::default()).unwrap();
    let request = CheckRequest {
        document: draft.clone(),
        set: None,
    };
    let checked = match engine.check_document(&request, &Cancellation::default()).unwrap().value {
        CheckResult::Checked(checked) => checked,
        refused => panic!("{refused:?}"),
    };
    assert_eq!((checked.read.citations, checked.read.keys), (2, 2));
    assert_eq!(checked.read.findings.len(), 1);
    assert_eq!(checked.read.findings[0].key.as_deref(), Some("ghost"));

    // Without a child, nothing is parsed.
    let alone = Engine::new().unwrap().without_file_watches();
    assert_eq!(
        alone.check_document(&request, &Cancellation::default()).unwrap().value,
        CheckResult::Refused {
            file: ibis_engine::Which::Document,
            reason: Refusal::Unavailable
        }
    );
}

#[test]
fn the_child_refuses_a_damaged_request() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ibis"))
        .arg(CHILD_ARGUMENT)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&u64::MAX.to_le_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}
