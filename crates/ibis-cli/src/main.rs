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
    Toggle {
        /// Desktop app: macOS app name or .app path; executable path elsewhere.
        #[arg(long, value_name = "NAME_OR_PATH")]
        app: Option<PathBuf>,
    },
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
        Command::Toggle { app } => {
            let sibling = std::env::current_exe()
                .ok()
                .map(|exe| exe.with_file_name(format!("ibis-desktop{}", std::env::consts::EXE_SUFFIX)))
                .filter(|path| path.is_file());
            // Per-user (NSIS) first, then machine-wide (MSI or NSIS for all users).
            let install_roots: Vec<PathBuf> = ["LOCALAPPDATA", "ProgramFiles"]
                .into_iter()
                .filter_map(std::env::var_os)
                .map(PathBuf::from)
                .collect();
            let mut command = desktop_command(
                std::env::consts::OS,
                sibling.as_deref(),
                app.as_deref(),
                &install_roots,
                Path::is_file,
            );
            let failed =
                || anyhow::anyhow!("could not start Ibis; install the desktop app or run its executable with toggle");
            if cfg!(target_os = "macos") && (app.is_some() || sibling.is_none()) {
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
/// `toggle` to the running copy. Overrides win over sibling, Windows install
/// folders (in the order given) and PATH; macOS app selection uses Launch Services.
fn desktop_command(
    platform: &str,
    sibling: Option<&Path>,
    app: Option<&Path>,
    install_roots: &[PathBuf],
    mut is_file: impl FnMut(&Path) -> bool,
) -> std::process::Command {
    // Tauri installers: <root>\<productName>\<binary>, productName Ibis, binary ibis-desktop.
    let installed = install_roots
        .iter()
        .filter(|_| platform == "windows" && app.is_none() && sibling.is_none())
        .filter(|root| !root.as_os_str().is_empty())
        .map(|root| root.join("Ibis").join("ibis-desktop.exe"))
        .find(|path| is_file(path));
    let mut command = if platform == "macos" && (app.is_some() || sibling.is_none()) {
        let mut open = std::process::Command::new("open");
        // A new process is required for single-instance arguments to arrive.
        open.args(["-n", "-a"])
            .arg(app.unwrap_or(Path::new("Ibis")))
            .arg("--args");
        open
    } else {
        let binary = format!("ibis-desktop{}", if platform == "windows" { ".exe" } else { "" });
        std::process::Command::new(app.or(sibling).or(installed.as_deref()).unwrap_or(Path::new(&binary)))
    };
    command.arg("toggle");
    command
}

#[cfg(test)]
mod toggle_tests {
    use super::*;

    #[test]
    fn toggle_accepts_an_explicit_app() {
        assert!(Cli::try_parse_from(["ibis", "toggle", "--app", "Ibis Nightly"]).is_ok());
        assert!(Cli::try_parse_from(["ibis", "toggle"]).is_ok());
        for extra in ["extra", "--at-login", "--app", "--app="] {
            assert!(Cli::try_parse_from(["ibis", "toggle", extra]).is_err());
        }
        let sibling = Some(Path::new("/synthetic/sibling"));
        for app in ["Ibis Nightly", "/synthetic/文献 $(x).app"] {
            let command = desktop_command("macos", sibling, Some(Path::new(app)), &[], |_| false);
            assert_eq!(command.get_program(), "open");
            let expected = ["-n", "-a", app, "--args", "toggle"];
            assert_eq!(command.get_args().collect::<Vec<_>>(), expected);
        }
    }

    #[test]
    fn toggle_launches_the_desktop_with_one_literal_argument() {
        let path = Path::new("/synthetic/Ibis app/ibis-desktop");
        let command = desktop_command("linux", Some(path), None, &[], |_| false);
        assert_eq!(command.get_program(), path);
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["toggle"]);
        let command = desktop_command("macos", None, None, &[], |_| false);
        let expected = ["-n", "-a", "Ibis", "--args", "toggle"];
        assert_eq!(command.get_args().collect::<Vec<_>>(), expected);
    }

    #[test]
    fn toggle_windows_lookup_prefers_override_then_sibling_then_install_folders_then_path() {
        let user = PathBuf::from("/synthetic/Local 文献");
        let machine = PathBuf::from("/synthetic/Program Files");
        let in_user = user.join("Ibis").join("ibis-desktop.exe");
        let in_machine = machine.join("Ibis").join("ibis-desktop.exe");
        let explicit = Path::new("/synthetic/Nightly app.exe");
        let sibling = Path::new("/synthetic/sibling.exe");
        let fallback = Path::new("ibis-desktop.exe");
        let roots = [user.clone(), machine.clone()];
        for (sibling, app, roots, existing, expected) in [
            (None, None, &roots[..], vec![&in_user, &in_machine], in_user.as_path()),
            (None, None, &roots[..], vec![&in_machine], in_machine.as_path()),
            (None, None, &roots[..], vec![], fallback),
            (None, None, &[][..], vec![&in_user], fallback),
            (None, None, &[PathBuf::new()][..], vec![], fallback),
            (Some(sibling), None, &roots[..], vec![&in_user], sibling),
            (Some(sibling), Some(explicit), &roots[..], vec![&in_user], explicit),
        ] {
            let command = desktop_command("windows", sibling, app, roots, |path| {
                assert!(sibling.is_none() && app.is_none());
                existing.iter().any(|p| p.as_path() == path)
            });
            assert_eq!(command.get_program(), expected);
            assert_eq!(command.get_args().collect::<Vec<_>>(), ["toggle"]);
        }
    }
}
