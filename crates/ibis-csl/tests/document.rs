#![allow(clippy::unwrap_used)]
use ibis_core::contract::*;
use ibis_csl::{DocumentCluster, DocumentRequest, DocumentResult, FormatError, Formatter, Limits};
use serde_json::{Value, json};
use std::process::{Command, Stdio};
use std::sync::Arc;

fn reference(key: &str) -> ItemRef {
    ItemRef {
        library: LibraryId::digest(b"synthetic-document-library"),
        key: key.into(),
    }
}
fn fixtures() -> (Snapshot, Vec<DocumentRequest>) {
    let cases: Value = serde_json::from_str(include_str!("document-cases.json")).unwrap();
    let records = cases["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|csl| {
            LibraryObject::Record(
                serde_json::from_value(json!({"id":reference(csl["id"].as_str().unwrap()),"csl":csl})).unwrap(),
            )
        })
        .collect();
    let documents = cases["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| DocumentRequest {
            style: case["style"].as_str().unwrap().into(),
            locale: Some(case["locale"].as_str().unwrap().into()),
            clusters: case["clusters"]
                .as_array()
                .unwrap()
                .iter()
                .map(|cluster| DocumentCluster {
                    items: cluster["ids"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|id| serde_json::from_value(json!({"item":reference(id.as_str().unwrap())})).unwrap())
                        .collect(),
                    note_index: cluster["note_index"].as_u64().unwrap() as u32,
                })
                .collect(),
        })
        .collect();
    (Snapshot::new(records).unwrap(), documents)
}
fn oracle() -> Vec<DocumentResult> {
    let output = Command::new("node")
        .arg(format!("{}/tests/document-oracle.mjs", env!("CARGO_MANIFEST_DIR")))
        .env("NODE_OPTIONS", "")
        .env("NODE_USE_SYSTEM_CA", "0")
        .stdin(Stdio::null())
        .output()
        .expect("the independent document oracle must execute");
    assert!(
        output.status.success(),
        "oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn complete_documents_match_the_direct_processor_and_do_not_share_state() {
    let (snapshot, documents) = fixtures();
    let formatter = Formatter::new().unwrap();
    let actual: Vec<_> = documents
        .iter()
        .map(|document| formatter.format_document(&snapshot, document).unwrap())
        .collect();
    assert_eq!(actual, oracle());
    assert_eq!(actual[0].clusters, ["[1]", "[2]", "[1]"]);
    assert_eq!(actual[1].clusters, ["(Taylor, 2026b)", "(Taylor, 2026a)"]);
    assert!(actual[2].clusters[0].contains("Synthetic Zeta"));
    assert_eq!(actual[2].clusters[1], "Ibid.");
    assert!(actual[2].clusters[3].contains("Zeta"));
    assert_ne!(actual[2].clusters[0], actual[2].clusters[3]);
    assert_eq!(actual[3].clusters, ["[1]"]);
    assert_eq!(actual[4].clusters, ["(Taylor, 2026)"]);
    for result in &actual[..3] {
        assert_eq!(result.bibliography.len(), 2);
        let entries = result.bibliography.join("\n");
        assert_eq!(entries.matches("Synthetic Zeta").count(), 1);
        assert_eq!(entries.matches("Synthetic Alpha").count(), 1);
        assert!(!entries.contains("Uncited"));
    }
}

#[test]
fn refusal_is_atomic_and_the_next_document_succeeds() {
    let (snapshot, documents) = fixtures();
    let formatter = Formatter::new().unwrap();
    let mut request = documents[0].clone();
    request.clusters[1].items[0].item.key = "missing".into();
    assert_eq!(
        formatter.format_document(&snapshot, &request),
        Err(FormatError::InvalidRequest)
    );
    request = documents[0].clone();
    request.clusters[0].items[0].form = Some(CitationForm::Full);
    assert_eq!(
        formatter.format_document(&snapshot, &request),
        Err(FormatError::InvalidRequest)
    );
    request = documents[0].clone();
    request.locale = Some("invented-locale".into());
    assert_eq!(
        formatter.format_document(&snapshot, &request),
        Err(FormatError::InvalidLocale)
    );
    assert_eq!(
        formatter.format_document(&snapshot, &documents[0]).unwrap().clusters,
        ["[1]", "[2]", "[1]"]
    );
}

#[test]
fn cancellation_and_limits_never_publish_a_partial_document() {
    let (snapshot, documents) = fixtures();
    let snapshot = Arc::new(snapshot);
    let formatter = Formatter::new().unwrap();
    let task = formatter
        .submit_document(snapshot.clone(), documents[2].clone())
        .unwrap();
    task.cancel();
    assert_eq!(task.wait(), Err(FormatError::Cancelled));
    assert_eq!(
        formatter.format_document(&snapshot, &documents[3]).unwrap().clusters,
        ["[1]"]
    );
    for (limits, expected) in [
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
        assert_eq!(
            Formatter::with_limits(limits)
                .unwrap()
                .format_document(&snapshot, &documents[0]),
            Err(expected)
        );
    }
}
