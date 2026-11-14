#![allow(clippy::unwrap_used)]
use ibis_core::contract::*;
use ibis_csl::{DocumentCluster, DocumentRequest, DocumentResult, FormatError, Formatter, Limits};
use serde_json::{Value, json};
use std::process::{Command, Stdio};
use std::sync::Arc;

fn fields(index: usize) -> Value {
    json!({"id":index.to_string(), "type":"book", "title":if index == 0 {"Alpha signals"} else {"Beta signals"},
        "author":[{"family":"Taylor","given":"Avery"}], "issued":{"date-parts":[[2026]]}, "publisher":"Imaginary Press"})
}
fn reference(index: usize) -> ItemRef {
    ItemRef {
        library: LibraryId::digest(b"invented-document-library"),
        key: index.to_string(),
    }
}
fn snapshot() -> Snapshot {
    let records = (0..2)
        .map(|index| {
            let mut csl = fields(index);
            csl["URL"] = json!("file:///home/invented/private.pdf");
            csl["note"] = json!("Invented private note");
            LibraryObject::Record(serde_json::from_value(json!({"id":reference(index),"csl":csl})).unwrap())
        })
        .collect();
    Snapshot::new(records).unwrap()
}
fn citation(index: usize) -> CitationItem {
    serde_json::from_value(json!({"item":reference(index)})).unwrap()
}
fn document(style: &str, notes: &[u32]) -> DocumentRequest {
    DocumentRequest {
        style: style.into(),
        locale: Some("en-US".into()),
        clusters: [1, 0, 1]
            .into_iter()
            .zip(notes)
            .map(|(index, note)| DocumentCluster {
                items: vec![citation(index)],
                note_index: *note,
            })
            .collect(),
    }
}
fn oracle(request: &DocumentRequest) -> DocumentResult {
    let mut child = Command::new("node")
        .arg(format!("{}/tests/document-oracle.mjs", env!("CARGO_MANIFEST_DIR")))
        .env("NODE_OPTIONS", "")
        .env("NODE_USE_SYSTEM_CA", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    serde_json::to_writer(
        child.stdin.take().unwrap(),
        &json!({"document":request,"items":[fields(0),fields(1)]}),
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn ieee_numbers_follow_the_document_and_repeated_references_keep_their_number() {
    let formatter = Formatter::new().unwrap();
    let mut request = document("ieee", &[0, 0, 0]);
    let result = formatter.format_document(&snapshot(), &request).unwrap();
    assert_eq!(result.clusters, ["[1]", "[2]", "[1]"]);
    assert_eq!(result.bibliography.lines().count(), 2);
    assert_eq!(result, oracle(&request));
    assert_eq!(formatter.format_document(&snapshot(), &request).unwrap(), result);
    request.clusters[2].items.push(citation(0));
    request.clusters[2].items[1].locator = Some(Locator {
        term: LocatorTerm::Page,
        value: "12".into(),
    });
    assert_eq!(
        formatter.format_document(&snapshot(), &request).unwrap(),
        oracle(&request)
    );
}

#[test]
fn later_same_author_year_work_updates_earlier_clusters_and_the_bibliography() {
    let formatter = Formatter::new().unwrap();
    let request = document("apa", &[0, 0, 0]);
    let mut first = request.clone();
    first.clusters.truncate(1);
    assert_eq!(
        formatter.format_document(&snapshot(), &first).unwrap().clusters,
        ["(Taylor, 2026)"]
    );
    let result = formatter.format_document(&snapshot(), &request).unwrap();
    assert_eq!(
        result.clusters,
        ["(Taylor, 2026b)", "(Taylor, 2026a)", "(Taylor, 2026b)"]
    );
    assert!(result.bibliography.contains("2026a") && result.bibliography.contains("2026b"));
    assert_eq!(result, oracle(&request));
}

#[test]
fn explicit_note_positions_control_repeated_note_formatting() {
    let formatter = Formatter::new().unwrap();
    let mut adjacent = document("chicago-notes-bibliography-16th-edition", &[1, 1, 2]);
    adjacent.clusters.remove(1);
    let mut separated = adjacent.clone();
    separated.clusters[1].note_index = 8;
    let a = formatter.format_document(&snapshot(), &adjacent).unwrap();
    let b = formatter.format_document(&snapshot(), &separated).unwrap();
    assert_ne!(a.clusters[0], a.clusters[1]);
    assert_ne!(a.clusters[1], b.clusters[1]);
    assert_eq!(a, oracle(&adjacent));
    assert_eq!(b, oracle(&separated));
}

#[test]
fn unknown_item_refuses_the_whole_document_and_isolated_citations_remain_unchanged() {
    let formatter = Formatter::new().unwrap();
    let snapshot = snapshot();
    let isolated: CitationRequest = serde_json::from_value(json!({"items":[citation(1)],
        "form":"normal","keep_order":false,"output":{"kind":"csl","style":"ieee"}}))
    .unwrap();
    let before = formatter.format(&snapshot, &isolated).unwrap();
    let mut request = document("ieee", &[0, 0, 0]);
    formatter.format_document(&snapshot, &request).unwrap();
    request.clusters[1].items[0].item = reference(99);
    assert_eq!(
        formatter.format_document(&snapshot, &request),
        Err(FormatError::InvalidRequest)
    );
    assert_eq!(formatter.format(&snapshot, &isolated).unwrap(), before);
    assert!(!before.text.contains("[1]"));
}

#[test]
fn document_tasks_share_cancellation_deadlines_and_output_limits() {
    let formatter = Formatter::new().unwrap();
    let request = document("apa", &[0, 0, 0]);
    let task = formatter
        .submit_document(Arc::new(snapshot()), request.clone())
        .unwrap();
    task.cancel();
    assert_eq!(task.wait(), Err(FormatError::Cancelled));
    assert!(formatter.format_document(&snapshot(), &request).is_ok());
    for (limits, error) in [
        (
            Limits {
                timeout: std::time::Duration::from_nanos(1),
                ..Limits::default()
            },
            FormatError::TimedOut,
        ),
        (
            Limits {
                output_bytes: 1,
                ..Limits::default()
            },
            FormatError::ResourceLimit,
        ),
    ] {
        let formatter = Formatter::with_limits(limits).unwrap();
        assert_eq!(formatter.format_document(&snapshot(), &request), Err(error));
    }
}
