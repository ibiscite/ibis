//! The public engine's output must survive the upstream converter, offline.
#![allow(clippy::unwrap_used)]
#[path = "support/zotero.rs"]
#[allow(dead_code)]
mod fixture;

use ibis_core::contract::{CitationError, CitationRequest, ItemOutcome};
use ibis_engine::{Cancellation, Engine, Method, request::ZoteroLibrary};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

#[test]
fn live_citation_handoff_uses_the_upstream_oracle() {
    let server = fixture::Server::new();
    let engine = Engine::with_zotero_port(server.port).unwrap().without_file_watches();
    let cancel = Cancellation::default();
    let open = |libraries| {
        engine
            .dispatch(
                Method::Libraries,
                &json!({"action":"open-zotero", "libraries":libraries}),
                &cancel,
            )
            .unwrap();
    };
    open(json!(["personal", {"group":7}]));
    let libraries = engine.libraries().unwrap().libraries;
    let snapshot = engine.snapshot().unwrap();
    let item = |group| {
        let origin = if group {
            ZoteroLibrary::Group(7)
        } else {
            ZoteroLibrary::Personal
        };
        let library = libraries.iter().find(|info| info.zotero == Some(origin)).unwrap();
        snapshot
            .objects()
            .iter()
            .find(|o| o.object.id().library == library.id)
            .unwrap()
            .object
            .id()
            .clone()
    };
    let mut cases: Vec<Value> =
        serde_json::from_str(include_str!("../../../fixtures/odf-scan/scenarios.json")).unwrap();
    for case in &mut cases {
        // Pandoc key lookup is checked with one unambiguous personal-library key.
        if case["syntax"] == "pandoc" {
            open(json!(["personal"]));
        }
        let items: Vec<_> = case["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|options| {
                let mut options = options.clone();
                let group = options.as_object_mut().unwrap().remove("group").unwrap_or(json!(false));
                options["item"] = json!(item(group.as_bool().unwrap()));
                options
            })
            .collect();
        let request: CitationRequest = serde_json::from_value(json!({
            "items":items, "keep_order":true,
            "output":{"kind":"key", "syntax":case["syntax"]}
        }))
        .unwrap();
        let result = engine.cite(&request, &cancel).unwrap();
        assert!(
            result
                .outcomes
                .iter()
                .all(|o| matches!(o, ItemOutcome::Rendered { .. })),
            "{}: {:?}",
            case["name"],
            result.outcomes
        );
        case["text"] = json!(result.text);
    }
    oracle(&cases);
}

#[test]
fn file_only_records_cannot_acquire_zotero_identity() {
    // A file may claim a Zotero URL, key and library number; none is provenance.
    let cancel = Cancellation::default();
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("invented.json");
    std::fs::write(&file, r#"[{"id":"SYNTH234","type":"book","title":"Invented impostor","citation-key":"Synthetic2026","URL":"http://zotero.org/groups/7/items/SYNTH234","libraryID":7}]"#).unwrap();
    let files = Engine::new().unwrap().without_file_watches();
    files.open_file_observed(&file, &cancel).unwrap();
    let snapshot = files.snapshot().unwrap();
    let request: CitationRequest = serde_json::from_value(json!({
        "items":[{"item":snapshot.objects()[0].object.id()}],
        "output":{"kind":"key","syntax":"scannable-cite"}
    }))
    .unwrap();
    let refused = files.cite(&request, &cancel).unwrap();
    assert_eq!(
        refused.outcomes,
        [ItemOutcome::Refused {
            item: 0,
            error: CitationError::NotFromZotero
        }]
    );
    assert!(refused.text.is_empty());
}

fn oracle(cases: &[Value]) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut child = Command::new("node")
        .arg(root.join("scripts/odf-scan.mjs"))
        .env("NODE_OPTIONS", "")
        .env("NODE_USE_SYSTEM_CA", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(cases).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "upstream ODF Scan differential failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    print!("{}", String::from_utf8_lossy(&output.stdout));
}
