//! A synthetic file through the real SDK and the real `ibis cite` parser.
use ibis_core::contract::{CitationRequest, CitationResult, LibraryObject};
use ibis_engine::{Cancellation, Engine, Method, request::SearchRequest};
use serde_json::json;
use std::io::Cursor;

#[test]
fn file_preview_and_native_canonical_copy_equal_ibis_cite() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("synthetic.bib");
    std::fs::write(
        &path,
        "@article{demo,title={Synthetic research},author={Researcher, Synthetic},year={2026}}\n",
    )?;
    let engine = Engine::new().map_err(|_| "engine")?;
    engine
        .dispatch(
            Method::Libraries,
            &json!({"action":"open-file","path":path}),
            &Cancellation::default(),
        )
        .map_err(|_| "open synthetic file")?;
    let rows = engine
        .search(
            &SearchRequest {
                query: "research".into(),
                limit: 10,
                libraries: None,
            },
            &Cancellation::default(),
        )
        .map_err(|_| "search synthetic file")?;
    assert_eq!(rows.hits.len(), 1);
    let item = &rows.hits[0].item;
    let LibraryObject::Record(record) = &item.object else {
        return Err("record expected".into());
    };
    for (output, option, value, text) in [
        (
            json!({"kind":"key","syntax":"pandoc","command":null}),
            "--format",
            "pandoc",
            "[@demo]",
        ),
        (
            json!({"kind":"csl","style":"apa"}),
            "--style",
            "apa",
            "(Researcher, 2026)",
        ),
    ] {
        let request: CitationRequest = serde_json::from_value(json!({
            "items":[{"item":record.id,"expected_cite":item.fingerprints.cite}],
            "output":output,"form":"normal","keep_order":true
        }))?;
        let mut stdout = vec![];
        let mut stderr = vec![];
        assert_eq!(
            ibis::run_with(
                engine.clone(),
                ["ibis", "cite", "demo", option, value, "--json"]
                    .map(Into::into)
                    .to_vec(),
                Cursor::new([]),
                &mut stdout,
                &mut stderr,
                ibis::OutputMode::Plain
            ),
            0
        );
        assert!(stderr.is_empty());
        let envelope: serde_json::Value = serde_json::from_slice(&stdout)?;
        let expected: CitationResult = serde_json::from_value(envelope["data"].clone())?;
        let prepared = engine
            .prepare_citation(&request, &Cancellation::default())
            .map_err(|_| "native preview")?;
        assert_eq!(prepared.result(), &expected);
        assert_eq!(expected.text, text);
        let mut writes = 0;
        engine
            .commit_citation(&prepared, &expected.content_hash, |content| {
                writes += 1;
                assert_eq!(content.text, expected.text);
                assert_eq!(content.html, expected.html.as_deref());
            })
            .map_err(|_| "native canonical commit")?;
        assert_eq!(writes, 1);
    }
    Ok(())
}
