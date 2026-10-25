//! `ibis` command line: search libraries, render citations, check files.
//!
//! A thin layer over `ibis-core`. Search, cite and check accept `--json` for
//! scripts; csl always emits JSON. Exit codes: 0 success, 1 error, 2 problems found
//! (by `check`).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use ibis_core::cite::{self, CiteItem, KeyFormat, Locator, LocatorLabel};
use ibis_core::library::{self, SelectError};
use ibis_core::model::{LoadResult, Reference};
use ibis_core::search::OwnedIndex;
use ibis_core::source::load_library_file;

#[derive(Parser)]
#[command(name = "ibis", version, about = "Find references and cite them anywhere.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show or hide the desktop picker, starting Ibis if needed.
    Toggle,
    /// Search references by words from the title, authors, year, key, keywords or abstract.
    Search {
        /// What to look for, e.g. "garcia sleep 2022".
        query: String,
        /// BibTeX/BibLaTeX file(s) to search. Repeat for several files.
        #[arg(long = "bib", required = true)]
        bib: Vec<PathBuf>,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        /// Print search results and load warnings as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Write a citation for one or more keys in a source-document syntax.
    Cite {
        /// Citation keys, in order.
        #[arg(required_unless_present = "items")]
        keys: Vec<String>,
        /// Read a JSON array of citation items from stdin (locators, notes, author omission).
        #[arg(long, conflicts_with_all = ["keys", "pages"])]
        items: bool,
        #[arg(long, value_enum, default_value_t = Format::Pandoc)]
        format: Format,
        /// Page locator for a key, e.g. `--page smith2020=12-14`. Repeatable.
        /// PAGES must contain non-whitespace text.
        #[arg(long = "page", value_name = "KEY=PAGES")]
        pages: Vec<String>,
        /// Check that every key exists in these files first.
        #[arg(long = "bib")]
        bib: Vec<PathBuf>,
        /// Print the citation and any warnings as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Report problems in BibTeX/BibLaTeX files without changing them.
    Check {
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Print per-file counts and warnings as JSON.
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
            eprintln!("ibis: {}", terminal_safe(&format!("{err:#}")));
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
            Ok(Loaded {
                display: path.display().to_string(),
                result: load_library_file(path, false)?,
            })
        })
        .collect()
}

/// Tell the user when a file loaded with problems, so skipped entries are never invisible.
fn report_problems(loaded: &[Loaded]) {
    for file in loaded.iter().filter(|f| !f.result.warnings.is_empty()) {
        eprintln!(
            "warning: {} has {} problem(s); some entries or fields were not loaded. Run `ibis check {}` for details.",
            terminal_safe(&file.display),
            file.result.warnings.len(),
            terminal_safe(&file.display)
        );
    }
}

/// Move every file's references out instead of copying them; warnings and
/// names stay with each file for reporting.
fn take_references(loaded: &mut [Loaded]) -> Vec<Reference> {
    loaded
        .iter_mut()
        .flat_map(|f| std::mem::take(&mut f.result.references))
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

/// Make library text safe to print on a terminal: control characters and
/// invisible bidirectional overrides (which can hide or reorder text) are
/// shown as visible escapes instead of being interpreted.
fn terminal_safe(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        let bidi_control = matches!(c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}');
        if c.is_control() || bidi_control {
            out.push_str(&c.escape_unicode().to_string());
        } else {
            out.push(c);
        }
    }
    out
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command::Toggle => {
            let sibling = std::env::current_exe()
                .ok()
                .map(|exe| exe.with_file_name(format!("ibis-desktop{}", std::env::consts::EXE_SUFFIX)))
                .filter(|path| path.is_file());
            let mut command = desktop_command(sibling.as_deref());
            let failed =
                || anyhow::anyhow!("could not start Ibis; install the desktop app or run its executable with toggle");
            if cfg!(target_os = "macos") && sibling.is_none() {
                if !command.status().map_err(|_| failed())?.success() {
                    return Err(failed());
                }
            } else {
                command.spawn().map_err(|_| failed())?;
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Search {
            query,
            bib,
            limit,
            json,
        } => {
            let mut loaded = load(&bib)?;
            let index = OwnedIndex::new(take_references(&mut loaded));
            let hits = index.search(&query, limit);
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
                        // Organizations and "et al." are literal names; others show the family name.
                        .map(|n| {
                            if n.literal.is_some() {
                                n.display()
                            } else {
                                n.family.clone().unwrap_or_default()
                            }
                        })
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
                        terminal_safe(r.citekey.as_deref().unwrap_or("-")),
                        terminal_safe(&authors),
                        terminal_safe(r.title_plain().as_deref().unwrap_or("[no title]"))
                    );
                    println!("{:<24} matched: {why}", "");
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Cite {
            keys,
            items: structured,
            format,
            pages,
            bib,
            json,
        } => {
            let mut items = if structured {
                cite::read_items(std::io::stdin().lock())?
            } else {
                keys.iter()
                    .map(|key| CiteItem {
                        key: key.clone(),
                        ..CiteItem::default()
                    })
                    .collect()
            };
            let mut load_warnings = Vec::new();
            if !bib.is_empty() {
                let mut loaded = load(&bib)?;
                if !json {
                    report_problems(&loaded);
                }
                load_warnings = all_warnings(&loaded);
                let references = take_references(&mut loaded);
                for item in &items {
                    if structured {
                        library::find_by_key(&references, &item.key).map_err(|error| match error {
                            SelectError::NotFound(_) => {
                                anyhow::anyhow!("a requested citation key was not found in the given files")
                            }
                            SelectError::Ambiguous { .. } | SelectError::DuplicateKeys { .. } => anyhow::anyhow!(
                                "a requested citation key is ambiguous; pass only one matching library with --bib"
                            ),
                        })?;
                    } else {
                        resolve(&item.key, &references)?;
                    }
                }
            }
            for spec in &pages {
                let Some((key, value)) = spec.rsplit_once('=').filter(|(_, value)| !value.trim().is_empty()) else {
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
            let rendered = cite::render(format.into(), &items).map_err(|error| match error {
                cite::CiteError::InvalidKey(_) if structured => {
                    anyhow::anyhow!("a citation key in the request is invalid")
                }
                cite::CiteError::KeyNotValidFor(_, syntax) if structured => {
                    anyhow::anyhow!("a citation key cannot be used in {syntax}")
                }
                other => other.into(),
            })?;
            if json {
                print_json(&serde_json::json!({
                    "text": rendered.text,
                    "warnings": rendered.warnings,
                    "load_warnings": load_warnings,
                }))?;
            } else {
                // The citation is the command's product and its keys come from the
                // command line, so it is printed exactly; only messages are escaped.
                println!("{}", rendered.text);
                for warning in &rendered.warnings {
                    eprintln!("note: {}", terminal_safe(warning));
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
                        terminal_safe(&file.display),
                        file.result.references.len(),
                        file.result.warnings.len()
                    );
                    for w in &file.result.warnings {
                        let line = w.line.map_or_else(String::new, |l| format!("line {l}: "));
                        println!("  {line}{}", terminal_safe(&w.message));
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
            let mut loaded = load(&bib)?;
            report_problems(&loaded);
            let references = take_references(&mut loaded);
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

/// Launch the same desktop binary again; its single-instance plugin forwards
/// `toggle` to the running copy. A sibling supports source builds and Windows
/// installs; otherwise use PATH, or Launch Services for the macOS app bundle.
fn desktop_command(sibling: Option<&Path>) -> std::process::Command {
    let mut command = if let Some(path) = sibling {
        std::process::Command::new(path)
    } else if cfg!(target_os = "macos") {
        let mut open = std::process::Command::new("open");
        // A new process is required for single-instance arguments to arrive.
        open.args(["-n", "-a", "Ibis", "--args"]);
        open
    } else {
        std::process::Command::new(format!("ibis-desktop{}", std::env::consts::EXE_SUFFIX))
    };
    command.arg("toggle");
    command
}

#[cfg(test)]
mod toggle_tests {
    use super::*;

    #[test]
    fn toggle_is_a_standalone_command() {
        assert!(Cli::try_parse_from(["ibis", "toggle"]).is_ok());
        for args in [vec!["ibis", "toggle", "extra"], vec!["ibis", "toggle", "--at-login"]] {
            assert!(Cli::try_parse_from(args).is_err());
        }
    }

    #[test]
    fn toggle_launches_the_desktop_with_one_literal_argument() {
        let path = Path::new("/synthetic/Ibis app/ibis-desktop");
        let command = desktop_command(Some(path));
        assert_eq!(command.get_program(), path);
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["toggle"]);
        let command = desktop_command(None);
        let expected = if cfg!(target_os = "macos") {
            vec!["-n", "-a", "Ibis", "--args", "toggle"]
        } else {
            vec!["toggle"]
        };
        assert_eq!(command.get_args().collect::<Vec<_>>(), expected);
    }
}
