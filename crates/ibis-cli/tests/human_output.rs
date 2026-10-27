use ibis_core::contract::*;
use ibis_engine::Engine;
use std::{io::Cursor, sync::Arc};

fn engine() -> Result<Engine, Box<dyn std::error::Error>> {
    let case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json"))?;
    Ok(Engine::from_snapshot(Arc::new(case.library)))
}

fn run(engine: Engine, args: &[&str]) -> Result<(u8, String, String), Box<dyn std::error::Error>> {
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let code = ibis::run_with(
        engine,
        args.iter().map(|argument| (*argument).into()).collect(),
        Cursor::new([]),
        &mut output,
        &mut errors,
        ibis::OutputMode::Plain,
    );
    Ok((code, String::from_utf8(output)?, String::from_utf8(errors)?))
}

#[test]
fn search_and_libraries_have_short_human_summaries() -> Result<(), Box<dyn std::error::Error>> {
    let (code, text, errors) = run(engine()?, &["ibis", "search", "signals"])?;
    assert_eq!(code, 0);
    assert!(errors.is_empty());
    assert_eq!(text, "Example2026\tAvery Taylor\t2026\tSynthetic signals\n");
    let (code, text, errors) = run(engine()?, &["ibis", "libraries"])?;
    assert_eq!(code, 0);
    assert!(errors.is_empty());
    assert!(text.starts_with("1 library\n"), "{text}");
    assert!(text.contains("1 item"), "{text}");
    assert!(!text.contains("fingerprints"));

    let (_, json, errors) = run(engine()?, &["ibis", "search", "signals", "--json"])?;
    let envelope: serde_json::Value = serde_json::from_str(&json)?;
    assert_eq!(envelope["ibis"], 1);
    assert_eq!(envelope["ok"], true);
    assert_eq!(
        envelope["data"]["hits"][0]["item"]["object"]["data"]["csl"]["title"],
        "Synthetic signals"
    );
    assert!(errors.is_empty());
    Ok(())
}

#[test]
fn human_result_rows_cannot_inject_extra_lines_or_terminal_controls() -> Result<(), Box<dyn std::error::Error>> {
    let case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json"))?;
    let LibraryObject::Record(mut record) = case.library.objects()[0].object.clone() else {
        return Err("synthetic record".into());
    };
    record.csl.insert(
        "title".into(),
        serde_json::json!("Synthetic\n\t\u{1b}[2J\u{202e}signals"),
    );
    let engine = Engine::from_snapshot(Arc::new(Snapshot::new(vec![LibraryObject::Record(record)])?));
    let (_, text, _) = run(engine, &["ibis", "search", "synthetic"])?;
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(!text.contains(['\u{1b}', '\u{202e}']));
    assert!(text.contains("\\u{1b}"));
    assert!(text.contains("\\u{202e}"));
    Ok(())
}

#[test]
fn empty_large_and_keyless_searches_remain_explicit() -> Result<(), Box<dyn std::error::Error>> {
    let (_, text, _) = run(engine()?, &["ibis", "search", "absent"])?;
    assert_eq!(text, "No references found.\n");
    let empty = Engine::from_snapshot(Arc::new(Snapshot::new(vec![])?));
    let (_, text, _) = run(empty, &["ibis", "libraries"])?;
    assert!(text.contains("No libraries loaded"));

    let case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json"))?;
    let LibraryObject::Record(mut record) = case.library.objects()[0].object.clone() else {
        return Err("synthetic record".into());
    };
    record.citation_keys.clear();
    record.csl.insert(
        "title".into(),
        serde_json::json!(format!("Synthetic {}", "文".repeat(100_000))),
    );
    let engine = Engine::from_snapshot(Arc::new(Snapshot::new(vec![LibraryObject::Record(record)])?));
    let (code, text, _) = run(engine, &["ibis", "search", "synthetic"])?;
    assert_eq!(code, 0);
    assert_eq!(text.lines().count(), 1);
    assert!(text.starts_with("[no citation key]\t"));
    assert_eq!(text.matches('文').count(), 100_000);
    Ok(())
}
