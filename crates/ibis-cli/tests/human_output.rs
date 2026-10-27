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
        errors.contains("note: two of these works share author and year"),
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
    assert_eq!(code, 1);
    assert!(text.is_empty(), "{text}");
    assert_eq!(
        errors,
        "ibis: cannot cite 'first' in apa with --form narrative; try --form normal.\n"
    );
    let (code, text, errors) = run_file(&path, &["cite", "--style", "invented", "first"])?;
    assert_eq!(code, 1);
    assert!(text.is_empty());
    assert_eq!(
        errors,
        "ibis: there is no style invented; list the styles with: ibis styles\n"
    );
    let (code, text, _) = run_file(
        &path,
        &["cite", "--style", "apa", "--form", "narrative", "first", "--json"],
    )?;
    assert_eq!(code, 1);
    let envelope: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(envelope["data"]["outcomes"][0]["error"], "unsupported-form");
    Ok(())
}

#[test]
fn errors_name_the_key_file_or_argument_and_keep_their_exit_codes() -> Result<(), Box<dyn std::error::Error>> {
    let path = library_file("errors.bib", SAME_AUTHOR_YEAR)?;
    let (code, text, errors) = run_file(&path, &["cite", "first", "absent"])?;
    assert_eq!((code, text.as_str()), (4, ""));
    assert_eq!(
        errors,
        "ibis: no reference has the citation key 'absent'; find the right key with: ibis search <words>\n"
    );
    let (code, _, errors) = run_file(&path, &["show", "absent"])?;
    assert_eq!(code, 4);
    assert!(errors.contains("'absent'"), "{errors}");

    // One file drops a repeated key with a warning; two open files can share one.
    let one = library_file("duplicate-one.bib", "@book{same,title={Synthetic one},year={2026}}\n")?;
    let two = library_file("duplicate-two.bib", "@book{same,title={Synthetic two},year={2026}}\n")?;
    let (code, _, errors) = run_file(&one, &["--file", two.to_str().ok_or("path")?, "cite", "same"])?;
    assert_eq!(code, 3);
    assert!(
        errors.contains("several references use the citation key 'same'"),
        "{errors}"
    );

    let (code, _, errors) = run(engine()?, &["ibis", "--file", "/invented/missing.bib", "search", "x"])?;
    assert_eq!(code, 5);
    assert_eq!(
        errors,
        "ibis: cannot open /invented/missing.bib: there is no file at that path; check the spelling or give the full path.\n"
    );
    let folder = env!("CARGO_TARGET_TMPDIR");
    let (code, _, errors) = run(engine()?, &["ibis", "--file", folder, "search", "x"])?;
    assert_ne!(code, 0);
    assert!(errors.contains("it is a folder"), "{errors}");

    let snapshot = Arc::new(Snapshot::new(vec![])?);
    let empty = || Engine::from_snapshot(snapshot.clone());
    let (code, _, errors) = run(empty(), &["ibis", "show", "anything"])?;
    assert_eq!(code, 4);
    assert!(
        errors.contains("no library is open; add --file <library> or --zotero"),
        "{errors}"
    );
    let (_, text, _) = run(empty(), &["ibis", "search", "anything"])?;
    assert_eq!(text, "No library is open; add --file <library> or --zotero.\n");

    let (code, _, errors) = run(engine()?, &["ibis", "search", "x", "--limit", "5000"])?;
    assert_eq!(code, 3);
    assert_eq!(errors, "ibis: --limit must be 1000 or less.\n");
    let (code, _, errors) = run(engine()?, &["ibis", "call", "invented"])?;
    assert_eq!(code, 2);
    assert!(
        errors.contains("there is no method 'invented'; choose one of: libraries"),
        "{errors}"
    );
    let (code, _, errors) = run(engine()?, &["ibis", "call", "search", "--params", "{"])?;
    assert_eq!(code, 3);
    assert!(errors.contains("see: ibis schema search"), "{errors}");
    let (code, _, errors) = run(engine()?, &["ibis", "search"])?;
    assert_eq!(code, 2);
    assert!(errors.starts_with("ibis: the following required arguments"), "{errors}");
    assert!(errors.contains("<query>"), "{errors}");
    let (code, _, errors) = run(engine()?, &["ibis", "show", "--id", "nonsense"])?;
    assert_eq!(code, 3);
    assert!(errors.contains("--id must be an ItemRef"), "{errors}");

    let (code, text, errors) = run(engine()?, &["ibis", "show", "absent", "--json"])?;
    assert_eq!(code, 4);
    assert!(errors.is_empty());
    assert_eq!(text, "{\"error\":{\"code\":\"not-found\"},\"ibis\":1,\"ok\":false}\n");
    Ok(())
}

#[test]
fn libraries_name_the_file_people_gave() -> Result<(), Box<dyn std::error::Error>> {
    let path = library_file("named.bib", SAME_AUTHOR_YEAR)?;
    let (code, text, _) = run_file(&path, &["libraries"])?;
    assert_eq!(code, 0);
    assert_eq!(text, format!("1 library\n{}\t4 items\n", path.display()));
    let (_, text, _) = run(engine()?, &["ibis", "styles"])?;
    assert!(text.lines().any(|line| line == "apa\tAPA (7th edition)"), "{text}");
    Ok(())
}

#[test]
fn year_letters_notice_sees_the_same_year_and_names_however_they_are_written() -> Result<(), Box<dyn std::error::Error>>
{
    let work = |id: &str, extra: serde_json::Value| {
        let mut item = serde_json::json!({"id":id,"type":"book","title":format!("Synthetic {id}")});
        for (field, value) in extra.as_object().into_iter().flatten() {
            item[field] = value.clone();
        }
        item
    };
    let ng = serde_json::json!([{"family":"Ng","given":"Avery"}]);
    let library = serde_json::json!([
        work(
            "number",
            serde_json::json!({"author":ng,"issued":{"date-parts":[[2020]]}})
        ),
        work(
            "string",
            serde_json::json!({"author":ng,"issued":{"date-parts":[["2020"]]}})
        ),
        work("literal", serde_json::json!({"author":ng,"issued":{"literal":"2020"}})),
        work(
            "particle",
            serde_json::json!({"author":[{"family":"Berg","non-dropping-particle":"van"}],"issued":{"date-parts":[[2020]]}})
        ),
        work(
            "joined",
            serde_json::json!({"author":[{"family":"van Berg"}],"issued":{"date-parts":[[2020]]}})
        ),
        work(
            "film-one",
            serde_json::json!({"type":"motion_picture","director":ng,"issued":{"date-parts":[[2021]]}})
        ),
        work(
            "film-two",
            serde_json::json!({"type":"motion_picture","director":ng,"issued":{"date-parts":[[2021]]}})
        ),
        work("undated", serde_json::json!({"author":ng})),
        work("undated-too", serde_json::json!({"author":ng})),
        work(
            "later",
            serde_json::json!({"author":ng,"issued":{"date-parts":[[2022]]}})
        ),
    ]);
    let path = library_file("year-forms.json", &library.to_string())?;
    for (keys, shared) in [
        (["number", "string"], true),
        (["number", "literal"], true),
        (["particle", "joined"], true),
        (["film-one", "film-two"], true),
        (["undated", "undated-too"], true),
        (["number", "later"], false),
        (["number", "particle"], false),
    ] {
        let (code, _, errors) = run_file(&path, &["cite", "--style", "apa", keys[0], keys[1]])?;
        assert_eq!(code, 0, "{keys:?}: {errors}");
        assert_eq!(errors.contains("share author and year"), shared, "{keys:?}: {errors}");
    }
    Ok(())
}

#[test]
fn call_cite_explains_its_findings_too() -> Result<(), Box<dyn std::error::Error>> {
    let path = library_file("call-cite.bib", SAME_AUTHOR_YEAR)?;
    let (_, shown, _) = run_file(&path, &["show", "first", "--json"])?;
    let shown: serde_json::Value = serde_json::from_str(&shown)?;
    let params = serde_json::json!({
        "items":[{"item":shown["data"]["object"]["data"]["id"]}],
        "output":{"kind":"csl","style":"apa"},
        "form":"narrative",
    });
    let (code, text, errors) = run_file(&path, &["call", "cite", "--params", &params.to_string()])?;
    assert_eq!((code, text.as_str()), (1, ""));
    assert!(errors.contains("with --form narrative"), "{errors}");
    Ok(())
}
