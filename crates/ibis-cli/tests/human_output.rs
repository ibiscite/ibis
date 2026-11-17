use ibis_core::contract::*;
use ibis_engine::Engine;
use std::{io::Cursor, sync::Arc};

fn engine() -> Result<Engine, Box<dyn std::error::Error>> {
    let case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json"))?;
    Ok(Engine::from_snapshot(Arc::new(case.library)))
}

fn run(engine: Engine, args: &[&str]) -> Result<(u8, String, String), Box<dyn std::error::Error>> {
    run_mode(engine, args, ibis::OutputMode::Plain)
}

fn run_mode(
    engine: Engine,
    args: &[&str],
    mode: ibis::OutputMode,
) -> Result<(u8, String, String), Box<dyn std::error::Error>> {
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let code = ibis::run_with(
        engine,
        args.iter().map(|argument| (*argument).into()).collect(),
        Cursor::new([]),
        &mut output,
        &mut errors,
        mode,
    );
    Ok((code, String::from_utf8(output)?, String::from_utf8(errors)?))
}

#[test]
fn field_warnings_stay_in_libraries_and_name_the_entry_and_field() -> Result<(), Box<dyn std::error::Error>> {
    let path = library_file(
        "field-warning.bib",
        "@book{vale2024,title={Invented Lantern},date={2020-13-45x}}",
    )?;
    let engine = Engine::new().map_err(|_| "engine")?.without_file_watches();
    engine
        .open_file_observed(&path, &ibis_engine::Cancellation::default())
        .map_err(|_| "open")?;
    let expected = engine
        .dispatch(
            ibis_engine::Method::Search,
            &serde_json::json!({"query":"lantern","limit":10}),
            &ibis_engine::Cancellation::default(),
        )
        .map_err(|_| "search")?
        .data;
    let (_, _, errors) = run(engine.clone(), &["ibis", "search", "lantern"])?;
    assert!(errors.is_empty(), "{errors}");
    let (_, text, errors) = run_mode(engine.clone(), &["ibis", "libraries"], ibis::OutputMode::Terminal)?;
    assert!(errors.is_empty(), "{errors}");
    assert!(text.contains("vale2024:"), "{text}");
    assert!(text.contains("\"date\""), "{text}");
    assert!(text.contains("check the date before citing"), "{text}");
    let (_, json, _) = run(engine.clone(), &["ibis", "search", "lantern", "--json"])?;
    assert_eq!(serde_json::from_str::<serde_json::Value>(&json)?["data"], expected);
    let expected = serde_json::to_value(engine.libraries().map_err(|_| "libraries")?)?;
    let (_, json, _) = run(engine, &["ibis", "libraries", "--json"])?;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&json)?["data"]["libraries"],
        expected["libraries"]
    );
    Ok(())
}

#[test]
fn a_failed_refresh_prints_one_note_naming_the_library_and_retained_results() -> Result<(), Box<dyn std::error::Error>>
{
    let path = library_file("invented-retained.bib", "@book{vale2024,title={Invented Lantern}}")?;
    let engine = Engine::new().map_err(|_| "engine")?.without_file_watches();
    engine
        .open_file_observed(&path, &ibis_engine::Cancellation::default())
        .map_err(|_| "open")?;
    let library = engine.libraries().map_err(|_| "libraries")?.libraries.remove(0);
    std::fs::remove_file(&path)?;
    assert!(
        engine
            .dispatch(
                ibis_engine::Method::Libraries,
                &serde_json::json!({"action":"reload","library":library.id}),
                &ibis_engine::Cancellation::default()
            )
            .is_err()
    );
    let (code, text, errors) = run(engine, &["ibis", "search", "lantern"])?;
    assert_eq!(code, 1);
    assert!(text.contains("Invented Lantern"), "{text}");
    assert_eq!(errors.lines().count(), 1, "{errors}");
    assert!(
        errors.starts_with(&format!(
            "ibis: note: {} could not be refreshed; showing results from ",
            library.id.as_str()
        )),
        "{errors}"
    );
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
    for mode in [ibis::OutputMode::Plain, ibis::OutputMode::Terminal] {
        let (_, text, _) = run_mode(engine.clone(), &["ibis", "search", "synthetic"], mode)?;
        let text = text
            .replace("\x1b[1m", "")
            .replace("\x1b[2m", "")
            .replace("\x1b[0m", "");
        assert_eq!(text.lines().count(), 1, "{text}");
        assert!(!text.contains(['\u{1b}', '\u{202e}']));
        assert!(text.contains("\\u{1b}"));
        assert!(text.contains("\\u{202e}"));
    }
    Ok(())
}

fn library_file(name: &str, contents: &str) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&path, contents)?;
    Ok(path)
}

fn run_file(path: &std::path::Path, args: &[&str]) -> Result<(u8, String, String), Box<dyn std::error::Error>> {
    let path = path.to_str().ok_or("path")?;
    let mut all = vec!["ibis", "--file", path];
    all.extend_from_slice(args);
    run(Engine::new().map_err(|_| "engine")?, &all)
}

const SAME_AUTHOR_YEAR: &str = "@book{first,title={Synthetic first},author={Taylor, Avery},year={2026}}\n\
@book{second,title={Synthetic second},author={Taylor, A.},year={2026}}\n\
@book{other,title={Synthetic other},author={Morgan, Ann},year={2026}}\n\
@book{twin,title={Synthetic twin},author={Morgan, Ann},year={2025}}\n";

#[test]
fn year_letters_notice_appears_only_when_two_cited_works_share_author_and_year()
-> Result<(), Box<dyn std::error::Error>> {
    let path = library_file("year-letters.bib", SAME_AUTHOR_YEAR)?;
    let (code, text, errors) = run_file(&path, &["cite", "--style", "apa", "first", "second"])?;
    assert_eq!(code, 0);
    assert_eq!(text, "(Taylor, 2026, 2026)\n");
    assert!(
        errors.contains("note: Two of these works share author and year"),
        "{errors}"
    );
    for keys in [&["first"][..], &["first", "other"], &["other", "twin"]] {
        let mut args = vec!["cite", "--style", "apa"];
        args.extend_from_slice(keys);
        let (code, _, errors) = run_file(&path, &args)?;
        assert_eq!(code, 0);
        assert!(errors.is_empty(), "{keys:?}: {errors}");
    }
    let (_, _, errors) = run_file(&path, &["cite", "--style", "apa", "--bibliography", "first", "second"])?;
    assert!(errors.contains("share author and year"), "{errors}");
    Ok(())
}

#[test]
fn a_refused_citation_prints_its_reason_and_fails() -> Result<(), Box<dyn std::error::Error>> {
    let path = library_file("refused.bib", SAME_AUTHOR_YEAR)?;
    let (code, text, errors) = run_file(&path, &["cite", "--style", "apa", "--form", "narrative", "first"])?;
    assert_eq!((code, text.as_str(), errors.as_str()), (0, "Taylor (2026)\n", ""));
    let (code, text, errors) = run_file(
        &path,
        &[
            "cite",
            "--style",
            "apa",
            "--form",
            "narrative",
            "first",
            "--omit-author",
        ],
    )?;
    assert_eq!(code, 1);
    assert!(text.is_empty(), "{text}");
    assert_eq!(
        errors,
        "ibis: Cannot cite 'first' in APA with --form narrative. Try --form normal.\n"
    );
    // An unknown style is a wrong request, not a refused reference.
    let (code, text, errors) = run_file(&path, &["cite", "--style", "invented", "first"])?;
    assert_eq!(code, 3);
    assert!(text.is_empty());
    assert_eq!(
        errors,
        "ibis: there is no style 'invented'; list the styles with: ibis styles\n"
    );
    let (code, text, _) = run_file(
        &path,
        &[
            "cite",
            "--style",
            "apa",
            "--form",
            "narrative",
            "first",
            "--omit-author",
            "--json",
        ],
    )?;
    assert_eq!(code, 1);
    let envelope: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(envelope["data"]["outcomes"][0]["error"], "unsupported-form");
    Ok(())
}

/// One command as a person sees it: what they typed, the exit code and both streams.
fn transcript(entry: &mut String, typed: &str, (code, stdout, stderr): (u8, String, String)) {
    use std::fmt::Write;
    let folder = env!("CARGO_TARGET_TMPDIR");
    let _ = write!(entry, "$ ibis {typed}\nexit {code}\n");
    for (stream, text) in [("stdout", stdout), ("stderr", stderr)] {
        // Windows separates the file name with `\`; the transcript is the same on every system.
        for line in text
            .replace(&format!("{folder}\\"), "<tmp>/")
            .replace(folder, "<tmp>")
            .lines()
        {
            let _ = writeln!(entry, "{stream}| {line}");
        }
    }
    entry.push('\n');
}

/// Reviewed with `cargo insta review` (docs/testing.md); the wording is the contract.
#[test]
fn errors_name_the_key_file_or_argument_and_keep_their_exit_codes() -> Result<(), Box<dyn std::error::Error>> {
    let path = library_file("errors.bib", SAME_AUTHOR_YEAR)?;
    // One file drops a repeated key with a warning; two open files can share one.
    let one = library_file("duplicate-one.bib", "@book{same,title={Synthetic one},year={2026}}\n")?;
    let two = library_file("duplicate-two.bib", "@book{same,title={Synthetic two},year={2026}}\n")?;
    let two = two.to_str().ok_or("path")?;
    let folder = env!("CARGO_TARGET_TMPDIR");
    let snapshot = Arc::new(Snapshot::new(vec![])?);
    let empty = || Engine::from_snapshot(snapshot.clone());
    let mut said = String::new();
    transcript(
        &mut said,
        "--file errors.bib cite first absent",
        run_file(&path, &["cite", "first", "absent"])?,
    );
    transcript(
        &mut said,
        "--file errors.bib show absent",
        run_file(&path, &["show", "absent"])?,
    );
    transcript(
        &mut said,
        "--file duplicate-one.bib --file duplicate-two.bib cite same",
        run_file(&one, &["--file", two, "cite", "same"])?,
    );
    for (typed, engine, args) in [
        (
            "--file /invented/missing.bib search x",
            engine()?,
            &["ibis", "--file", "/invented/missing.bib", "search", "x"][..],
        ),
        (
            "--file <tmp> search x",
            engine()?,
            &["ibis", "--file", folder, "search", "x"],
        ),
        ("show anything (no library)", empty(), &["ibis", "show", "anything"]),
        ("search anything (no library)", empty(), &["ibis", "search", "anything"]),
        (
            "search x --limit 5000",
            engine()?,
            &["ibis", "search", "x", "--limit", "5000"],
        ),
        ("call invented", engine()?, &["ibis", "call", "invented"]),
        (
            "call search --params {",
            engine()?,
            &["ibis", "call", "search", "--params", "{"],
        ),
        ("search", engine()?, &["ibis", "search"]),
        ("show --id nonsense", engine()?, &["ibis", "show", "--id", "nonsense"]),
        ("show absent --json", engine()?, &["ibis", "show", "absent", "--json"]),
    ] {
        transcript(&mut said, typed, run(engine, args)?);
    }
    insta::assert_snapshot!(said);
    // Scripts and agents get the stable code on stdout and the same sentence on stderr.
    let (_, _, plain) = run(engine()?, &["ibis", "show", "absent"])?;
    let (_, _, errors) = run(engine()?, &["ibis", "show", "absent", "--json"])?;
    assert_eq!(errors, plain);
    Ok(())
}

#[test]
fn a_line_break_in_any_argument_cannot_forge_a_diagnostic() -> Result<(), Box<dyn std::error::Error>> {
    for json in [false, true] {
        for args in [
            vec!["ibis", "search", "x", "--limit=bad\nibis: forged"],
            vec!["ibis", "search", "x", "--limit", "bad\nibis: forged"],
            vec!["ibis", "cite", "k", "--format=bad\r\nibis: forged"],
            vec!["ibis", "invented\nibis: forged"],
        ] {
            let mut args = args;
            if json {
                args.push("--json");
            }
            let (code, _, errors) = run(engine()?, &args)?;
            assert_eq!(code, 2, "{args:?}");
            let starts = errors.lines().filter(|line| line.starts_with("ibis: ")).count();
            assert_eq!(starts, 1, "{args:?}: {errors}");
            assert!(
                errors
                    .lines()
                    .skip(1)
                    .all(|line| line.is_empty() || line.starts_with("  ")),
                "{errors}"
            );
        }
    }
    Ok(())
}

#[test]
fn a_preloaded_engine_never_reads_the_app_settings() -> Result<(), Box<dyn std::error::Error>> {
    // With no library named it searches what it was given, never the app's choice.
    let (code, _, errors) = run(engine()?, &["ibis", "search", "x", "--json"])?;
    assert_eq!((code, errors.as_str()), (0, ""));
    // Asked for the app's libraries, it refuses instead of reading or ignoring them.
    let (code, text, errors) = run(engine()?, &["ibis", "search", "x", "--app-libraries", "--json"])?;
    assert_eq!(code, 2);
    assert_eq!(
        text,
        "{\"error\":{\"code\":\"usage\",\"focus\":\"request\",\"message\":{\"args\":{},\"id\":\"error-usage\"},\"recovery\":{\"action\":\"edit-input\"}},\"ibis\":1,\"ok\":false}\n"
    );
    assert!(errors.contains("--app-libraries is not available here"), "{errors}");
    // The server starts and says the same, so an agent can tell the person.
    let (code, _, errors) = run(engine()?, &["ibis", "mcp", "--app-libraries", "--json"])?;
    assert_eq!(code, 0);
    assert!(errors.contains("--app-libraries is not available here"), "{errors}");
    Ok(())
}
