//! `ibis` command line: search libraries, render citations, check files.
//!
//! A thin layer over `ibis-core`. Every command accepts `--json` for scripts,
//! editors and AI tools. Exit codes: 0 success, 1 error, 2 problems found
//! (by `check`).

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use ibis_core::bibtex;
use ibis_core::cite::{self, CiteItem, KeyFormat, Locator, LocatorLabel};
use ibis_core::library::{self, SelectError};
use ibis_core::model::{LoadResult, Reference};
use ibis_core::search::Index;

#[derive(Parser)]
#[command(name = "ibis", version, about = "Find references and cite them anywhere.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Search references by words from the title, authors, year, key, keywords or abstract.
    Search {
        /// What to look for, e.g. "garcia sleep 2022".
        query: String,
        /// BibTeX/BibLaTeX file(s) to search. Repeat for several files.
        #[arg(long = "bib", required = true)]
        bib: Vec<PathBuf>,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Write a citation for one or more keys in a source-document syntax.
    Cite {
        /// Citation keys, in order.
        #[arg(required = true)]
        keys: Vec<String>,
        #[arg(long, value_enum, default_value_t = Format::Pandoc)]
        format: Format,
        /// Page locator for a key, e.g. `--page smith2020=12-14`. Repeatable.
        #[arg(long = "page", value_name = "KEY=PAGES")]
        pages: Vec<String>,
        /// Check that every key exists in these files first.
        #[arg(long = "bib")]
        bib: Vec<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Report problems in BibTeX/BibLaTeX files without changing them.
    Check {
        #[arg(required = true)]
        files: Vec<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Print selected references as CSL-JSON (for Pandoc, citeproc, scripts).
    Csl {
        /// Citation keys to export.
        #[arg(required_unless_present = "all")]
        keys: Vec<String>,
        /// Export every reference in the given files.
        #[arg(long, conflicts_with = "keys")]
        all: bool,
        #[arg(long = "bib", required = true)]
        bib: Vec<PathBuf>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Pandoc,
    Autocite,
    Parencite,
    Textcite,
    Cite,
    Typst,
    Keys,
}

impl From<Format> for KeyFormat {
    fn from(f: Format) -> Self {
        match f {
            Format::Pandoc => KeyFormat::Pandoc,
            Format::Autocite => KeyFormat::Autocite,
            Format::Parencite => KeyFormat::Parencite,
            Format::Textcite => KeyFormat::Textcite,
            Format::Cite => KeyFormat::Cite,
            Format::Typst => KeyFormat::Typst,
            Format::Keys => KeyFormat::Keys,
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("ibis: {err:#}");
            ExitCode::from(1)
        }
    }
}

/// One loaded file. `source` is the canonical path, so two files with the
/// same name in different folders never share record identities.
struct Loaded {
    display: String,
    result: LoadResult,
}

fn load(files: &[PathBuf]) -> Result<Vec<Loaded>> {
    files
        .iter()
        .map(|path| {
            let text = std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
            let source = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            Ok(Loaded {
                display: path.display().to_string(),
                result: bibtex::load_str(&source.to_string_lossy(), &text),
            })
        })
        .collect()
}

/// Tell the user when a file loaded with problems, so skipped entries are never invisible.
fn report_problems(loaded: &[Loaded]) {
    for file in loaded.iter().filter(|f| !f.result.warnings.is_empty()) {
        eprintln!(
            "warning: {} has {} problem(s); some entries or fields were not loaded. Run `ibis check {}` for details.",
            file.display,
            file.result.warnings.len(),
            file.display
        );
    }
}

fn all_references(loaded: &[Loaded]) -> Vec<Reference> {
    loaded
        .iter()
        .flat_map(|f| f.result.references.iter().cloned())
        .collect()
}

fn all_warnings(loaded: &[Loaded]) -> Vec<serde_json::Value> {
    loaded
        .iter()
        .flat_map(|f| {
            f.result
                .warnings
                .iter()
                .map(|w| serde_json::json!({"file": f.display, "warning": w}))
        })
        .collect()
}

/// Find the one reference with this citation key, refusing to guess when
/// several files contain it.
fn resolve<'a>(key: &str, references: &'a [Reference]) -> Result<&'a Reference> {
    library::find_by_key(references, key).map_err(|err| match err {
        SelectError::Ambiguous { key, sources } => anyhow::anyhow!(
            "key `{key}` appears in several files ({}); pass only the file you mean with --bib",
            sources.join(", ")
        ),
        other => anyhow::anyhow!("{other} in the given files"),
    })
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command::Search {
            query,
            bib,
            limit,
            json,
        } => {
            let loaded = load(&bib)?;
            let references = all_references(&loaded);
            let hits = Index::new(&references).search(&query, limit);
            if !json {
                report_problems(&loaded);
            }
            if json {
                print_json(&serde_json::json!({"results": hits, "warnings": all_warnings(&loaded)}))?;
            } else if hits.is_empty() {
                println!("No matches.");
            } else {
                for hit in &hits {
                    let r = hit.reference;
                    let authors = r
                        .authors
                        .iter()
                        .map(|n| n.family.clone().unwrap_or_default())
                        .collect::<Vec<_>>()
                        .join(", ");
                    let year = r.year().map_or_else(|| "n.d.".to_string(), |y| y.to_string());
                    let why = hit
                        .matched
                        .iter()
                        .map(|m| format!("{m:?}").to_lowercase())
                        .collect::<Vec<_>>()
                        .join(", ");
                    println!(
                        "{:<24} {} ({year}) {}",
                        r.citekey.as_deref().unwrap_or("-"),
                        authors,
                        r.title_plain().as_deref().unwrap_or("[no title]")
                    );
                    println!("{:<24} matched: {why}", "");
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Cite {
            keys,
            format,
            pages,
            bib,
            json,
        } => {
            if !bib.is_empty() {
                let loaded = load(&bib)?;
                if !json {
                    report_problems(&loaded);
                }
                let references = all_references(&loaded);
                for key in &keys {
                    resolve(key, &references)?;
                }
            }
            let mut items: Vec<CiteItem> = keys
                .iter()
                .map(|k| CiteItem {
                    key: k.clone(),
                    ..CiteItem::default()
                })
                .collect();
            for spec in &pages {
                let Some((key, value)) = spec.rsplit_once('=') else {
                    bail!("--page needs KEY=PAGES, got `{spec}`")
                };
                let Some(item) = items.iter_mut().find(|i| i.key == key) else {
                    bail!("--page refers to `{key}`, which is not being cited")
                };
                item.locator = Some(Locator {
                    label: LocatorLabel::Page,
                    value: value.to_string(),
                });
            }
            let rendered = cite::render(format.into(), &items)?;
            if json {
                print_json(&rendered)?;
            } else {
                println!("{}", rendered.text);
                for warning in &rendered.warnings {
                    eprintln!("note: {warning}");
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Check { files, json } => {
            let loaded = load(&files)?;
            let problems: usize = loaded.iter().map(|f| f.result.warnings.len()).sum();
            if json {
                let report: Vec<_> = loaded
                    .iter()
                    .map(|f| serde_json::json!({"file": f.display, "loaded": f.result.references.len(), "warnings": f.result.warnings}))
                    .collect();
                print_json(&report)?;
            } else {
                for file in &loaded {
                    println!(
                        "{}: {} references loaded, {} problems",
                        file.display,
                        file.result.references.len(),
                        file.result.warnings.len()
                    );
                    for w in &file.result.warnings {
                        let line = w.line.map_or_else(String::new, |l| format!("line {l}: "));
                        println!("  {line}{}", w.message);
                    }
                }
            }
            Ok(if problems == 0 {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(2)
            })
        }
        Command::Csl { keys, all, bib } => {
            let loaded = load(&bib)?;
            report_problems(&loaded);
            let references = all_references(&loaded);
            let selected: Vec<&Reference> = if all {
                references.iter().collect()
            } else {
                keys.iter()
                    .map(|key| resolve(key, &references))
                    .collect::<Result<_>>()?
            };
            let items = library::csl_items(selected)
                .map_err(|err| anyhow::anyhow!("{err}; pass the files separately with --bib"))?;
            print_json(&items)?;
            Ok(ExitCode::SUCCESS)
        }
    }
}
