//! Invented files through the native picker bridge and the real `ibis cite` parser.
use crate::library::engine_file::{FileLibrary, FileSelection};
use ibis_core::{cite::KeyFormat, contract::CitationResult, settings::CitationStyle};
use ibis_engine::{Cancellation, Engine, Method};
use serde_json::json;
use std::io::Cursor;

#[test]
fn file_preview_and_native_canonical_copy_equal_ibis_cite() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    for (name, source) in [
        (
            "synthetic.bib",
            "@article{demo,title={Synthetic research},author={Researcher, Synthetic},year={2026}}\n",
        ),
        (
            "synthetic.json",
            r#"[{"id":"demo","citation-key":"demo","type":"article-journal","title":"Synthetic research","author":[{"family":"Researcher","given":"Synthetic"}],"issued":{"date-parts":[[2026]]}}]"#,
        ),
    ] {
        let path = directory.path().join(name);
        std::fs::write(&path, source)?;
        let file = FileLibrary::open(&path).map_err(|_| "native file open")?;
        let rows = serde_json::to_value(
            file.search(
                &serde_json::from_value(json!({"text":"research","interpret_locators":false}))?,
                10,
                None,
            )
            .map_err(|_| "native search")?,
        )?;
        assert_eq!(rows["results"].as_array().ok_or("native rows")?.len(), 1);
        let row = &rows["results"][0];
        let cli = Engine::new().map_err(|_| "CLI engine")?;
        cli.dispatch(
            Method::Libraries,
            &json!({"action":"open-file","path":path}),
            &Cancellation::default(),
        )
        .map_err(|_| "CLI file open")?;
        for (style, option, value, text) in [
            (None, "--format", "pandoc", "[@demo]"),
            (Some(CitationStyle::Apa), "--style", "apa", "(Researcher, 2026)"),
        ] {
            let mut stdout = vec![];
            let mut stderr = vec![];
            assert_eq!(
                ibis::run_with(
                    cli.clone(),
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
            let selection: FileSelection =
                serde_json::from_value(json!({"id":row["id"],"cite":row["cite"],"locator":null}))?;
            let preview = file
                .preview(file.reserve_preview(), vec![selection], KeyFormat::Pandoc, style)
                .map_err(|_| "native preview")?;
            assert_eq!(preview.text, expected.text);
            assert_eq!(preview.html, expected.html);
            assert_eq!(preview.content_hash, expected.content_hash.as_str());
            assert_eq!(preview.text, text);
            let mut writes = 0;
            let copied = file
                .copy(&preview.ticket, &preview.content_hash, |content| {
                    writes += 1;
                    assert_eq!(content.text, expected.text);
                    assert_eq!(content.html, expected.html.as_deref());
                    Ok(())
                })
                .map_err(|_| "native canonical commit")?;
            assert_eq!(writes, 1);
            assert_eq!(copied.text, expected.text);
            for (ticket, hash) in [
                ("forged", preview.content_hash.as_str()),
                (preview.ticket.as_str(), "forged"),
            ] {
                assert!(
                    file.copy(ticket, hash, |_| panic!("a forged receipt cannot write"))
                        .is_err()
                );
            }
        }
    }
    Ok(())
}

#[test]
fn page_locators_and_formatted_order_match_the_real_cli() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("synthetic.bib");
    std::fs::write(
        &path,
        "@book{z,title={Synthetic Zulu},author={Zulu, Zoe},year={2026}}\n@book{a,title={Synthetic Alpha},author={Alpha, Amy},year={2026}}",
    )?;
    let file = FileLibrary::open(&path).map_err(|_| "native file")?;
    let rows = serde_json::to_value(
        file.search(
            &serde_json::from_value(json!({"text":"synthetic","interpret_locators":false}))?,
            10,
            None,
        )
        .map_err(|_| "native search")?,
    )?;
    let mut rows = rows["results"].as_array().ok_or("rows")?.clone();
    rows.sort_by(|a, b| b["key"].as_str().cmp(&a["key"].as_str()));
    let cli = Engine::new().map_err(|_| "CLI")?;
    cli.dispatch(
        Method::Libraries,
        &json!({"action":"open-file","path":path}),
        &Cancellation::default(),
    )
    .map_err(|_| "CLI file")?;
    for (style, locator, count) in [
        (None, true, 1),
        (Some(CitationStyle::Apa), true, 1),
        (Some(CitationStyle::Apa), false, 2),
        (Some(CitationStyle::Ieee), false, 1),
    ] {
        let output = match style {
            Some(style) => json!({"kind":"csl","style":style}),
            None => json!({"kind":"key","syntax":"pandoc","command":null}),
        };
        let request = json!({"items": rows[..count].iter().map(|row| json!({"item":row["id"],"expected_cite":row["cite"],"locator":locator.then(|| json!({"term":"page","value":"12"}))})).collect::<Vec<_>>(), "output":output,"form":if style == Some(CitationStyle::Ieee) {"bibliography-only"} else {"normal"},"keep_order":style.is_none()});
        let mut stdout = vec![];
        let mut stderr = vec![];
        assert_eq!(
            ibis::run_with(
                cli.clone(),
                ["ibis", "cite", "--request", "--json"].map(Into::into).to_vec(),
                Cursor::new(serde_json::to_vec(&request)?),
                &mut stdout,
                &mut stderr,
                ibis::OutputMode::Plain
            ),
            0
        );
        let envelope: serde_json::Value = serde_json::from_slice(&stdout)?;
        let expected: CitationResult = serde_json::from_value(envelope["data"].clone())?;
        let selections=rows[..count].iter().map(|row| serde_json::from_value(json!({"id":row["id"],"cite":row["cite"],"locator":locator.then(|| json!({"label":"page","value":"12"}))}))).collect::<Result<Vec<FileSelection>,_>>()?;
        let preview = file
            .preview(file.reserve_preview(), selections, KeyFormat::Pandoc, style)
            .map_err(|e| format!("native preview: {e}"))?;
        assert_eq!(preview.text, expected.text);
        assert_eq!(preview.html, expected.html);
        assert_eq!(preview.content_hash, expected.content_hash.as_str());
        assert!(
            preview.warnings.is_empty(),
            "an ordinary formatted citation must not invent an order warning"
        );
        file.copy(&preview.ticket, &preview.content_hash, |content| {
            assert_eq!(content.text, expected.text);
            assert_eq!(content.html, expected.html.as_deref());
            Ok(())
        })
        .map_err(|_| "native copy")?;
    }
    Ok(())
}

#[test]
fn malformed_file_selections_are_not_retryable_snapshot_errors() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("synthetic.bib");
    std::fs::write(&path, "@book{a,title={Synthetic}}")?;
    let file = FileLibrary::open(&path).map_err(|_| "file")?;
    let rows = serde_json::to_value(
        file.search(
            &serde_json::from_value(json!({"text":"synthetic","interpret_locators":false}))?,
            10,
            None,
        )
        .map_err(|_| "search")?,
    )?;
    let row = &rows["results"][0];
    for (id, cite) in [
        (json!({"source":"legacy","native_key":"a"}), row["cite"].clone()),
        (row["id"].clone(), json!("invalid-fingerprint")),
    ] {
        let selection = serde_json::from_value(json!({"id":id,"cite":cite,"locator":null}))?;
        let Err(error) = file.preview(file.reserve_preview(), vec![selection], KeyFormat::Pandoc, None) else {
            return Err("invalid selection accepted".into());
        };
        assert!(
            !matches!(error, crate::library::CommandError::SnapshotChanged),
            "malformed input cannot trigger an automatic status/preview loop"
        );
    }
    Ok(())
}
