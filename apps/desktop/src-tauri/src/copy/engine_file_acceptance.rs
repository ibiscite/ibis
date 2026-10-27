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
                .preview(vec![selection], KeyFormat::Pandoc, style)
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
