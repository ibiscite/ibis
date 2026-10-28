//! Review regressions travel from an invented BibTeX source to CLI output.
use ibis_core::contract::*;
use ibis_engine::Engine;
use serde_json::{Value, json};
use std::{io::Cursor, path::Path};

#[test]
fn citation_intent_and_literal_affixes_survive_the_source_and_cli_boundaries() -> Result<(), Box<dyn std::error::Error>>
{
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/formatted/citation-review.bib");
    let engine = Engine::new().map_err(|_| "engine")?;
    let mut output = vec![];
    let mut errors = vec![];
    assert_eq!(
        ibis::run_with(
            engine.clone(),
            vec![
                "ibis".into(),
                "search".into(),
                "".into(),
                "--file".into(),
                path.to_str().ok_or("path")?.into(),
                "--json".into()
            ],
            Cursor::new(vec![]),
            &mut output,
            &mut errors,
            ibis::OutputMode::Plain,
        ),
        0
    );
    let snapshot = engine.snapshot().map_err(|_| "snapshot")?;
    assert_eq!(snapshot.objects().len(), 1);
    let item = snapshot.objects()[0].object.id();
    let requests = [
        (
            json!({"items":[{"item":item},{"item":item,"form":"narrative"}],"output":{"kind":"key","syntax":"myst"}}),
            1,
            "{cite:p}`Example2026`",
            Some(1),
            None,
        ),
        (
            json!({"items":[{"item":item,"locator":{"term":"page","value":"4"},"suffix":[{"text":"12 more"}]}],"output":{"kind":"key","syntax":"org-cite"}}),
            1,
            "[cite:@Example2026, p. 4]",
            None,
            Some(LossField::Suffix),
        ),
        (
            json!({"items":[{"item":item,"prefix":[{"text":"$$ ","italic":true}],"suffix":[{"text":" $&","italic":true}]}],"output":{"kind":"csl","style":"apa"}}),
            0,
            "($$ Taylor, 2026 $&)",
            None,
            None,
        ),
        (
            json!({"items":[{"item":item,"suppress_author":true}],"form":"narrative","output":{"kind":"csl","style":"apa"}}),
            1,
            "",
            Some(0),
            None,
        ),
    ];
    for (request, code, text, refused, loss) in requests {
        output.clear();
        errors.clear();
        assert_eq!(
            ibis::run_with(
                engine.clone(),
                ["ibis", "cite", "--request", "--json"].map(Into::into).to_vec(),
                Cursor::new(serde_json::to_vec(&request)?),
                &mut output,
                &mut errors,
                ibis::OutputMode::Plain,
            ),
            code,
            "{request}"
        );
        assert!(errors.is_empty());
        let envelope: Value = serde_json::from_slice(&output)?;
        let result: CitationResult = serde_json::from_value(envelope["data"].clone())?;
        assert_eq!(result.text, text);
        if let Some(item) = refused {
            assert_eq!(
                result.outcomes[item],
                ItemOutcome::Refused {
                    item,
                    error: CitationError::UnsupportedForm
                }
            );
            assert!(
                result
                    .copy_content(request["items"].as_array().ok_or("items")?.len())
                    .is_none()
            );
        }
        assert_eq!(
            result.losses.iter().map(|loss| loss.field).collect::<Vec<_>>(),
            loss.into_iter().collect::<Vec<_>>()
        );
        if code == 0 {
            assert!(result.copy_content(1).is_some());
            assert!(result.html.as_deref().ok_or("HTML")?.contains("<i> $&#38;</i>"));
            assert!(result.rtf.as_deref().ok_or("RTF")?.contains("$&"));
        }
    }
    Ok(())
}
