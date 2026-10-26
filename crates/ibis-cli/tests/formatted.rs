use ibis_core::contract::{GoldenCase, Snapshot};
use ibis_engine::Engine;
use serde_json::Value;
use std::{io::Cursor, sync::Arc};

fn synthetic_engine() -> Result<Engine, Box<dyn std::error::Error>> {
    let case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json"))?;
    let snapshot: Snapshot = case.library;
    Ok(Engine::from_snapshot(Arc::new(snapshot)))
}

#[test]
fn human_style_option_formats_with_the_shared_engine() -> Result<(), Box<dyn std::error::Error>> {
    for (bibliography, expected) in [
        (false, "(Taylor, 2026)"),
        (true, "Taylor, A. (2026). Synthetic signals. Imaginary Press."),
    ] {
        let mut args = vec!["ibis", "cite", "Example2026", "--style", "apa", "--json"];
        if bibliography {
            args.push("--bibliography");
        }
        let mut output = vec![];
        let mut errors = vec![];
        let code = ibis::run_with(
            synthetic_engine()?,
            args.into_iter().map(Into::into).collect(),
            Cursor::new(vec![]),
            &mut output,
            &mut errors,
            ibis::OutputMode::Plain,
        );
        assert_eq!(code, 0);
        assert!(errors.is_empty());
        let result: Value = serde_json::from_slice(&output)?;
        assert_eq!(result["data"]["text"], expected);
        assert!(result["data"]["html"].is_string());
        assert!(result["data"]["rtf"].is_string());
    }
    Ok(())
}

fn formatted_cases() -> Result<Vec<GoldenCase>, serde_json::Error> {
    [
        include_str!("../../../fixtures/formatted/apa-citation.json"),
        include_str!("../../../fixtures/formatted/apa-bibliography.json"),
        include_str!("../../../fixtures/formatted/ieee-document-context.json"),
    ]
    .into_iter()
    .map(serde_json::from_str)
    .collect()
}
fn canonical(value: &Value) -> Result<Vec<u8>, ibis_engine::protocol::FrameError> {
    ibis_engine::wire::encode(value, ibis_engine::protocol::MAX_RESULT_BYTES)
}
#[test]
fn formatted_goldens_replay_through_engine_human_call_and_stdio() -> Result<(), Box<dyn std::error::Error>> {
    use ibis_core::contract::*;
    use ibis_engine::{
        Cancellation,
        protocol::{read_frame, write_frame},
    };
    for case in formatted_cases()? {
        let engine = Engine::from_snapshot(Arc::new(case.library));
        let expected = serde_json::to_value(case.expected.data().ok_or("expected result")?)?;
        let actual = engine
            .cite(&case.request, &Cancellation::default())
            .map_err(|_| "engine citation")?;
        assert_eq!(
            canonical(&serde_json::to_value(actual)?)?,
            canonical(&expected)?,
            "{}",
            case.name
        );
        let OutputSpec::Csl { style } = &case.request.output else {
            return Err("style".into());
        };
        let mut human = vec!["ibis", "cite", "Example2026", "--style", style, "--json"];
        if case.request.form == CitationForm::BibliographyOnly {
            human.push("--bibliography");
        }
        let request = serde_json::to_vec(&case.request)?;
        for args in [
            human,
            vec!["ibis", "cite", "--request", "--json"],
            vec!["ibis", "call", "cite", "--stdin", "--json"],
        ] {
            let mut output = vec![];
            let mut errors = vec![];
            assert_eq!(
                ibis::run_with(
                    engine.clone(),
                    args.into_iter().map(Into::into).collect(),
                    Cursor::new(request.clone()),
                    &mut output,
                    &mut errors,
                    ibis::OutputMode::Plain
                ),
                0,
                "{}",
                case.name
            );
            assert!(errors.is_empty());
            let mut expected_bytes = canonical(&serde_json::to_value(&case.expected)?)?;
            expected_bytes.push(b'\n');
            assert_eq!(output, expected_bytes, "{}", case.name);
        }
        let mut input = vec![];
        write_frame(
            &mut input,
            &serde_json::json!({"jsonrpc":"2.0","id":7,"method":"cite","params":case.request}),
        )?;
        let mut output = vec![];
        let mut errors = vec![];
        assert_eq!(
            ibis::run_with(
                engine,
                vec!["ibis".into(), "serve".into(), "--stdio".into()],
                Cursor::new(input),
                &mut output,
                &mut errors,
                ibis::OutputMode::Plain
            ),
            0
        );
        assert!(errors.is_empty());
        let mut frames = Cursor::new(output);
        let mut replies = 0;
        while let Some(frame) = read_frame(&mut frames)? {
            if frame.get("id") == Some(&serde_json::json!(7)) {
                replies += 1;
                assert_eq!(canonical(&frame["result"])?, canonical(&expected)?, "{}", case.name);
            }
        }
        assert_eq!(replies, 1);
    }
    Ok(())
}

#[test]
fn human_whole_document_fallback_explains_its_canonical_reference() -> Result<(), Box<dyn std::error::Error>> {
    let case = formatted_cases()?.pop().ok_or("fallback case")?;
    let expected = case.expected.data().ok_or("result")?.text.clone() + "\n";
    let mut output = vec![];
    let mut errors = vec![];
    assert_eq!(
        ibis::run_with(
            Engine::from_snapshot(Arc::new(case.library)),
            ["ibis", "cite", "Example2026", "--style", "ieee"]
                .into_iter()
                .map(Into::into)
                .collect(),
            Cursor::new(vec![]),
            &mut output,
            &mut errors,
            ibis::OutputMode::Plain
        ),
        0
    );
    assert_eq!(output, expected.as_bytes());
    let notices = String::from_utf8(errors)?;
    assert!(notices.contains("whole document"));
    assert!(notices.contains("reference entry"));
    Ok(())
}

#[test]
fn formatted_failures_stay_explicit_and_uncopyable() -> Result<(), Box<dyn std::error::Error>> {
    use ibis_core::contract::*;
    for (kind, expected) in [
        ("missing", CitationError::MissingItem),
        ("style", CitationError::UnsupportedOutput),
        ("nonprinting", CitationError::UnsupportedForm),
    ] {
        let mut case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json"))?;
        case.request.output = OutputSpec::Csl {
            style: if kind == "style" {
                "unknown-synthetic-style"
            } else {
                "modern-language-association"
            }
            .into(),
        };
        if kind == "missing" {
            case.request.items[0].item.key = "missing-synthetic-record".into();
        }
        if kind == "nonprinting" {
            let LibraryObject::Record(mut record) = case.library.objects()[0].object.clone() else {
                return Err("record".into());
            };
            record.csl = serde_json::from_value(serde_json::json!({"type":"book"}))?;
            case.library = Snapshot::new(vec![LibraryObject::Record(record)])?;
            case.request.items[0].expected_cite = None;
        }
        let mut output = vec![];
        let mut errors = vec![];
        let code = ibis::run_with(
            Engine::from_snapshot(Arc::new(case.library)),
            ["ibis", "cite", "--request", "--json"]
                .into_iter()
                .map(Into::into)
                .collect(),
            Cursor::new(serde_json::to_vec(&case.request)?),
            &mut output,
            &mut errors,
            ibis::OutputMode::Plain,
        );
        assert_eq!(code, 1, "{kind}");
        assert!(errors.is_empty());
        let result: Envelope<CitationResult> = serde_json::from_slice(&output)?;
        let citation = result.data().ok_or("citation result")?;
        assert_eq!(
            citation.outcomes,
            vec![ItemOutcome::Refused {
                item: 0,
                error: expected
            }],
            "{kind}"
        );
        assert!(citation.copy_content(1).is_none());
    }
    Ok(())
}
