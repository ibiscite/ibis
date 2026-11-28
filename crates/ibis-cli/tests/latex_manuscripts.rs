//! Ibis's BibTeX for every invented item of the Zotero mapping corpus
//! (fixtures/zotero/mapping), compiled into a LaTeX manuscript with biblatex and
//! Biber under pdfLaTeX, as Overleaf compiles one by default. The items are read
//! two ways: as Zotero items with citation keys, and as a CSL-JSON file (Ibis
//! without Zotero).
//!
//! Biber must finish, and every entry must reach the typeset bibliography with
//! its title. Everything else LaTeX or Biber says about an entry, and every
//! record Ibis refuses to write, is listed in `fixtures/latex/STYLE.json`. The
//! test fails when a list changes; review it like a snapshot, then run
//! `IBIS_BLESS=1 cargo test -p ibis --test latex_manuscripts`.
//!
//! It runs where TeX Live (pdfLaTeX, biblatex, biblatex-apa, Biber) and
//! Poppler's `pdftotext` are installed; `IBIS_REQUIRE_TEX=1` makes them required.
mod corpus;
use corpus::{Reader, TestResult, csl_json_reader, zotero_reader};
use ibis_core::contract::{CitationRequest, ItemOutcome};
use ibis_engine::Cancellation;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const FIXTURE: &str = "../../fixtures/latex";

fn tex_available() -> bool {
    let available = ["pdflatex", "biber", "pdftotext"].into_iter().all(|program| {
        Command::new(program)
            .arg("-v")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    });
    assert!(
        available || std::env::var_os("IBIS_REQUIRE_TEX").is_none(),
        "TeX Live and pdftotext are required"
    );
    available
}

/// Runs `program` in `dir`; false when it had not finished after `limit`.
fn run(dir: &Path, program: &str, args: &[&str], limit: Duration) -> TestResult<bool> {
    let mut child = Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let start = Instant::now();
    while child.try_wait()?.is_none() {
        if start.elapsed() > limit {
            child.kill()?;
            child.wait()?;
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(true)
}

/// A title as letters and digits only, so case, punctuation and markup
/// differences between a style and the record never count.
fn letters(text: &str) -> String {
    let mut plain = String::new();
    let mut tag = false;
    for c in text.chars() {
        match c {
            '<' => tag = true,
            '>' => tag = false,
            c if !tag && c.is_alphanumeric() => plain.extend(c.to_lowercase()),
            _ => {}
        }
    }
    plain
}

fn manuscript(style: &str) -> String {
    // Each entry opens with its key, so the typeset page can be read back
    // entry by entry; no hyphenation splits a word across lines.
    format!(
        r"\documentclass{{article}}
\usepackage[T1]{{fontenc}}
\usepackage{{lmodern}}
\usepackage[utf8]{{inputenc}}
\usepackage[paperwidth=60cm,paperheight=200cm,margin=1cm]{{geometry}}
\usepackage[backend=biber,style={style}]{{biblatex}}
\addbibresource{{refs.bib}}
\renewbibmacro*{{begentry}}{{\typeout{{ibis-entry:\thefield{{entrykey}}}}[[\thefield{{entrykey}}]] }}
\hyphenpenalty=10000 \exhyphenpenalty=10000
\begin{{document}}
\nocite{{*}}
\printbibliography
\end{{document}}
"
    )
}

/// One reader's entries in `style`: what it compiled and what was said.
fn compile(reader: &Reader, cases: &BTreeMap<String, Value>, style: &str, dir: &Path) -> TestResult<Vec<Value>> {
    let mut findings = BTreeSet::new();
    let mut say = |name: Option<&str>, finding: String| {
        findings.insert((name.unwrap_or_default().to_owned(), finding));
    };
    let mut bib = String::new();
    let mut names = BTreeMap::new();
    for (name, item) in &reader.items {
        let request: CitationRequest = serde_json::from_value(json!({
            "items": [item], "form": "normal", "output": {"kind": "key", "syntax": "bibtex"}
        }))?;
        let result = reader
            .engine
            .cite(&request, &Cancellation::default())
            .map_err(|error| format!("{name}: {:?}", error.code))?;
        match &result.outcomes[..] {
            [ItemOutcome::Rendered { .. }] => {
                let key = result.text.split_once('{').and_then(|(_, rest)| rest.split_once(','));
                names.insert(key.ok_or("entry key")?.0.to_owned(), name.clone());
                bib.push_str(&result.text);
                bib.push('\n');
            }
            outcomes => say(Some(name), format!("Ibis refuses: {outcomes:?}")),
        }
    }
    std::fs::write(dir.join("refs.bib"), bib)?;
    std::fs::write(dir.join("main.tex"), manuscript(style))?;
    let limit = Duration::from_secs(300);
    assert!(run(dir, "pdflatex", &["-interaction=nonstopmode", "main"], limit)?);
    assert!(
        run(dir, "biber", &["main"], limit)?,
        "{style}, {}: Biber did not finish",
        reader.name
    );
    assert!(run(dir, "pdflatex", &["-interaction=nonstopmode", "main"], limit)?);
    let name_of = |text: &str| -> Option<String> {
        names
            .iter()
            .find(|(key, _)| text.contains(&format!("'{key}'")) || text.contains(&format!("{{{key}}}")))
            .map(|(_, name)| name.clone())
    };
    // Biber's warnings and errors, each about one entry where it names one.
    for line in std::fs::read_to_string(dir.join("main.blg"))?.lines() {
        if let Some(at) = line.find("WARN - ").or_else(|| line.find("ERROR - ")) {
            let message = &line[at..];
            let mut message = message.to_owned();
            for (key, name) in &names {
                message = message.replace(&format!("'{key}'"), &format!("'{name}'"));
            }
            say(name_of(line).as_deref(), format!("Biber: {message}"));
        }
    }
    // LaTeX's errors, by the entry being typeset when they happened.
    let log = String::from_utf8_lossy(&std::fs::read(dir.join("main.log"))?).into_owned();
    let mut entry = None;
    for line in log.lines() {
        if let Some(key) = line.strip_prefix("ibis-entry:") {
            entry = names.get(key.trim()).cloned();
        } else if let Some(error) = line.strip_prefix("! ") {
            say(entry.as_deref(), format!("LaTeX: {error}"));
        }
    }
    // The typeset page, entry by entry: each must be there with its title.
    let output = Command::new("pdftotext")
        .args(["-enc", "UTF-8", "main.pdf", "-"])
        .current_dir(dir)
        .output()?;
    let page = String::from_utf8(output.stdout)?
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mut printed = BTreeMap::new();
    for part in page.split("[[").skip(1) {
        if let Some((key, text)) = part.split_once("]]") {
            printed.insert(key.to_owned(), text.to_owned());
        }
    }
    for (key, name) in &names {
        let title = cases[name]["expected"]["title"]
            .as_str()
            .map(letters)
            .unwrap_or_default();
        match printed.get(key) {
            None => say(Some(name), "not on the page".into()),
            Some(text) if !letters(text).contains(&title) => {
                say(Some(name), format!("title not on the page: {}", text.trim()))
            }
            Some(_) => {}
        }
    }
    Ok(findings
        .into_iter()
        .map(|(name, finding)| json!({"name": name, "reader": reader.name, "finding": finding}))
        .collect())
}

fn check(style: &str) -> TestResult {
    if !tex_available() {
        return Ok(());
    }
    let cases = corpus::cases()?;
    let by_name: BTreeMap<String, Value> = cases
        .iter()
        .map(|case| (case["name"].as_str().unwrap_or_default().to_owned(), case.clone()))
        .collect();
    let dir = tempfile::tempdir()?;
    let readers = [
        zotero_reader(&corpus::keyed(&cases))?,
        csl_json_reader(&cases, dir.path())?,
    ];
    let mut findings = vec![];
    let mut entries = BTreeMap::new();
    for reader in &readers {
        let here = dir.path().join(format!("{style}-{}", reader.name));
        std::fs::create_dir(&here)?;
        entries.insert(reader.name, reader.items.len());
        findings.extend(compile(reader, &by_name, style, &here)?);
    }
    let report = format!(
        "{{\"style\":{style:?},\"entries\":{},\"findings\":[\n{}\n]}}\n",
        json!(entries),
        findings.iter().map(Value::to_string).collect::<Vec<_>>().join(",\n")
    );
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURE)
        .join(format!("biblatex-{style}.json"));
    if std::env::var_os("IBIS_BLESS").is_some() {
        std::fs::write(&path, &report)?;
    }
    assert!(
        std::fs::read_to_string(&path).unwrap_or_default() == report,
        "{style}: what LaTeX and Biber say about Ibis's BibTeX changed; review it, then rerun with IBIS_BLESS=1"
    );
    Ok(())
}

#[test]
fn biblatex_authoryear_compiles_every_entry() -> TestResult {
    check("authoryear")
}

#[test]
fn biblatex_apa_compiles_every_entry() -> TestResult {
    check("apa")
}
