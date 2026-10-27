//! Target syntax generation after identity, intent and input validation.
use std::io::Write;

use serde::{Serialize, Serializer, ser::SerializeMap};

use super::escape::*;
use super::*;

pub(super) fn render(caps: &SyntaxCapabilities, picks: &[Pick<'_>]) -> Result<String, CitationError> {
    if picks.is_empty() {
        return Ok(String::new());
    }
    let text = match caps.syntax {
        KeySyntax::Pandoc => pandoc(picks),
        KeySyntax::Biblatex => biblatex(caps.command.unwrap_or("autocite"), picks),
        KeySyntax::Latex => latex(picks),
        KeySyntax::Typst => typst(picks),
        KeySyntax::Keys => picks.iter().map(|pick| pick.key).collect::<Vec<_>>().join(", "),
        KeySyntax::CslJson => return csl_json(picks),
        _ => return Err(CitationError::UnsupportedOutput),
    };
    Ok(text)
}

fn pandoc(picks: &[Pick<'_>]) -> String {
    let narrative = picks.iter().any(|pick| pick.form == Narrative);
    let parts = picks
        .iter()
        .map(|pick| {
            let prefix = rich(&pick.item.prefix, Escaping::Pandoc);
            let mut suffix = rich(&pick.item.suffix, Escaping::Pandoc);
            let locator = pick.item.locator.as_ref().map(|locator| {
                let text = pandoc_escape(&locator_text(locator, "-", " "));
                if suffix.is_empty() { text } else { format!("{{{text}}}") }
            });
            if locator.is_none() && !suffix.is_empty() {
                // Pandoc's explicit empty locator prevents literal numbers
                // (including styled numbers) from becoming page locators.
                suffix.insert_str(0, "{}");
            }
            let key = pandoc_key(pick.key);
            if pick.form == Narrative {
                let notes = locator
                    .into_iter()
                    .chain((!suffix.is_empty()).then_some(suffix))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "{}@{key}{}",
                    before(&prefix),
                    if notes.is_empty() {
                        String::new()
                    } else {
                        format!(" [{notes}]")
                    }
                )
            } else {
                let mut value = format!("{}{}@{key}", before(&prefix), if pick.suppress { "-" } else { "" });
                for note in locator.into_iter().chain((!suffix.is_empty()).then_some(suffix)) {
                    value.push_str(", ");
                    value.push_str(&note);
                }
                if narrative { format!("[{value}]") } else { value }
            }
        })
        .collect::<Vec<_>>()
        .join("; ");
    if narrative { parts } else { format!("[{parts}]") }
}

fn before(prefix: &str) -> String {
    if prefix.is_empty() {
        String::new()
    } else {
        format!("{prefix} ")
    }
}

fn biblatex(command: &str, picks: &[Pick<'_>]) -> String {
    let notes = |pick: &Pick<'_>| {
        let prefix = rich(&pick.item.prefix, Escaping::Latex);
        let suffix = rich(&pick.item.suffix, Escaping::Latex);
        let suffix = if pick.item.locator.is_none() && !suffix.is_empty() {
            // BibLaTeX otherwise treats numeric or Roman suffixes as pages.
            // Its prefix suppressor works anywhere in the postnote. Append it
            // so other TeX readers cannot consume a leading literal group.
            format!("{suffix}\\nopp")
        } else {
            suffix
        };
        let locator = pick.item.locator.as_ref().map(|locator| {
            if locator.term == LocatorTerm::Page {
                let value = latex_locator_escape(&normalize_dashes(locator.value.trim(), "--"));
                if suffix.is_empty() {
                    value
                } else {
                    // A mixed numeric/text postnote disables BibLaTeX's
                    // automatic page label; keep the typed page explicit.
                    format!(
                        "\\bibstring{{{}}}~{value}",
                        if is_range(&locator.value) { "pages" } else { "page" }
                    )
                }
            } else {
                latex_escape_locator(locator)
            }
        });
        let post = locator
            .into_iter()
            .chain((!suffix.is_empty()).then_some(suffix))
            .collect::<Vec<_>>()
            .join(", ");
        if !prefix.is_empty() {
            format!("[{prefix}][{post}]")
        } else if !post.is_empty() {
            format!("[{post}]")
        } else {
            String::new()
        }
    };
    if let [pick] = picks {
        format!(
            "\\{command}{}{}{{{}}}",
            if pick.suppress { "*" } else { "" },
            notes(pick),
            pick.key
        )
    } else {
        format!(
            "\\{command}s{}",
            picks
                .iter()
                .map(|pick| format!("{}{{{}}}", notes(pick), pick.key))
                .collect::<String>()
        )
    }
}

fn latex(picks: &[Pick<'_>]) -> String {
    if picks
        .iter()
        .all(|pick| pick.item.locator.is_none() && pick.item.prefix.is_empty() && pick.item.suffix.is_empty())
    {
        return format!(
            "\\cite{{{}}}",
            picks.iter().map(|pick| pick.key).collect::<Vec<_>>().join(",")
        );
    }
    picks
        .iter()
        .map(|pick| {
            let prefix = rich(&pick.item.prefix, Escaping::Latex);
            let suffix = rich(&pick.item.suffix, Escaping::Latex);
            let suffix = if pick.item.locator.is_none() && !suffix.is_empty() {
                // The document may define plain \cite through BibLaTeX.
                // A TeX primitive keeps this literal without a package macro.
                format!("{suffix}\\relax")
            } else {
                suffix
            };
            let post = pick
                .item
                .locator
                .as_ref()
                .map(latex_escape_locator)
                .into_iter()
                .chain((!suffix.is_empty()).then_some(suffix))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "{}\\cite{}{{{}}}",
                before(&prefix),
                if post.is_empty() {
                    String::new()
                } else {
                    format!("[{post}]")
                },
                pick.key
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn typst(picks: &[Pick<'_>]) -> String {
    picks
        .iter()
        .map(|pick| {
            let prefix = rich(&pick.item.prefix, Escaping::Typst);
            let suffix = rich(&pick.item.suffix, Escaping::Typst);
            let supplement = pick
                .item
                .locator
                .as_ref()
                .map(|locator| typst_literal(&locator_text(locator, "–", " "), false))
                .into_iter()
                .chain((!suffix.is_empty()).then_some(suffix))
                .collect::<Vec<_>>()
                .join(", ");
            let form = match pick.form {
                Narrative => Some("prose"),
                CitationForm::AuthorOnly => Some("author"),
                YearOnly => Some("year"),
                CitationForm::Full => Some("full"),
                _ if pick.suppress => Some("year"),
                _ => None,
            };
            // Explicit labels also preserve punctuation in consumers whose
            // shorthand reader accepts a narrower subset than Typst itself.
            let simple = typst_simple_label(pick.key) && !pick.key.contains([':', '.']);
            let body = if simple && form.is_none() {
                format!(
                    "@{}{}",
                    pick.key,
                    if supplement.is_empty() {
                        String::new()
                    } else {
                        format!("[{supplement}]")
                    }
                )
            } else {
                let label = if simple {
                    format!("<{}>", pick.key)
                } else {
                    format!("label({})", quoted(pick.key))
                };
                let mut args = vec![label];
                if !supplement.is_empty() {
                    args.push(format!("supplement: [{supplement}]"));
                }
                if let Some(form) = form {
                    args.push(format!("form: \"{form}\""));
                }
                format!("#cite({})", args.join(", "))
            };
            format!("{}{body}", before(&prefix))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

fn typst_literal(text: &str, isolate: bool) -> String {
    if isolate || text.contains(['~', '-', '\'', '"']) || text.contains("...") {
        // Shorthands, smart quotes and adjacent rich runs must stay literal.
        // A trusted text() call carries string data without reparsing markup.
        format!("#text({})", quoted(text))
    } else {
        typst_markup_escape(text)
    }
}

fn rich(runs: &RichText, escaping: Escaping) -> String {
    if runs.iter().all(|run| run.text.trim().is_empty()) {
        return String::new();
    }
    runs.iter()
        .map(|run| {
            let mut text = match escaping {
                Escaping::Pandoc => pandoc_affix_escape(&run.text),
                Escaping::Latex => latex_affix_escape(&run.text),
                Escaping::Typst => typst_literal(&run.text, runs.len() > 1),
                _ => run.text.clone(),
            };
            match escaping {
                Escaping::Pandoc => {
                    let trimmed = text.trim();
                    let leading = text.len() - text.trim_start().len();
                    let trailing = text.trim_end().len();
                    let mut body = trimmed.to_string();
                    if run.italic && !body.is_empty() {
                        body = format!("*{body}*");
                    }
                    if run.bold && !body.is_empty() {
                        body = format!("**{body}**");
                    }
                    if run.script != Script::Normal && !run.text.chars().any(char::is_whitespace) {
                        let mark = if run.script == Script::Subscript { '~' } else { '^' };
                        // A leading `[^` is a Pandoc footnote, including inside
                        // a rich span or narrative suffix bracket. An empty
                        // span separates the bracket without visible content.
                        body = format!("{}{mark}{body}{mark}", if mark == '^' { "[]{}" } else { "" });
                    }
                    if runs.len() > 1 && (run.italic || run.bold || run.script != Script::Normal) && !body.is_empty() {
                        // Empty-attribute spans isolate adjacent emphasis delimiters
                        // without inserting visible text into the citation.
                        body = format!("[{body}]{{}}");
                    }
                    text = if trimmed.is_empty() {
                        text
                    } else {
                        format!("{}{body}{}", &text[..leading], &text[trailing..])
                    };
                }
                Escaping::Latex => {
                    if run.italic {
                        text = format!("\\emph{{{text}}}");
                    }
                    if run.bold {
                        text = format!("\\textbf{{{text}}}");
                    }
                    if run.script != Script::Normal {
                        text = format!(
                            "\\text{}{{{text}}}",
                            if run.script == Script::Subscript {
                                "subscript"
                            } else {
                                "superscript"
                            }
                        );
                    }
                }
                Escaping::Typst => {
                    if run.italic {
                        text = format!("#emph[{text}]");
                    }
                    if run.bold {
                        text = format!("#strong[{text}]");
                    }
                    if run.script != Script::Normal {
                        text = format!(
                            "#{}[{text}]",
                            if run.script == Script::Subscript {
                                "sub"
                            } else {
                                "super"
                            }
                        );
                    }
                }
                _ => {}
            }
            text
        })
        .collect()
}

fn locator_text(locator: &Locator, dash: &str, space: &str) -> String {
    format!(
        "{}{space}{}",
        locator_label(locator.term, is_range(&locator.value)),
        normalize_dashes(locator.value.trim(), dash)
    )
}

fn latex_escape_locator(locator: &Locator) -> String {
    format!(
        "{}~{}",
        latex_escape(locator_label(locator.term, is_range(&locator.value))),
        latex_locator_escape(&normalize_dashes(locator.value.trim(), "--"))
    )
}

fn locator_label(term: LocatorTerm, plural: bool) -> &'static str {
    match term {
        LocatorTerm::Page => {
            if plural {
                "pp."
            } else {
                "p."
            }
        }
        LocatorTerm::Chapter => "chap.",
        LocatorTerm::Section => "sec.",
        LocatorTerm::Paragraph => "para.",
        LocatorTerm::Figure => "fig.",
        LocatorTerm::Line => {
            if plural {
                "ll."
            } else {
                "l."
            }
        }
        LocatorTerm::Note => "n.",
        LocatorTerm::Volume => "vol.",
        LocatorTerm::Verse => {
            if plural {
                "vv."
            } else {
                "v."
            }
        }
        LocatorTerm::Act => "act",
        LocatorTerm::Appendix => "appendix",
        LocatorTerm::ArticleLocator => "article",
        LocatorTerm::Book => "book",
        LocatorTerm::Canon => "canon",
        LocatorTerm::Column => "column",
        LocatorTerm::Elocation => "elocation",
        LocatorTerm::Equation => "equation",
        LocatorTerm::Folio => "folio",
        LocatorTerm::Issue => "issue",
        LocatorTerm::Opus => "opus",
        LocatorTerm::Part => "part",
        LocatorTerm::Rule => "rule",
        LocatorTerm::Scene => "scene",
        LocatorTerm::SubVerbo => "sub verbo",
        LocatorTerm::Supplement => "supplement",
        LocatorTerm::Table => "table",
        LocatorTerm::Timestamp => "timestamp",
        LocatorTerm::TitleLocator => "title",
        LocatorTerm::Version => "version",
    }
}

/// Serialize borrowed CSL fields directly into a bounded writer. Large repeated
/// records cannot create an unbounded series of cloned maps before the limit.
fn csl_json(picks: &[Pick<'_>]) -> Result<String, CitationError> {
    struct Export<'a>(&'a Pick<'a>);
    impl Serialize for Export<'_> {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut map = serializer.serialize_map(None)?;
            map.serialize_entry("id", self.0.key)?;
            map.serialize_entry("citation-key", self.0.key)?;
            for (key, value) in &self.0.record.csl {
                if key != "id" && key != "citation-key" {
                    map.serialize_entry(key, value)?;
                }
            }
            map.end()
        }
    }
    struct Bounded(Vec<u8>);
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > MAX_OUTPUT_BYTES.saturating_sub(self.0.len()) {
                return Err(std::io::ErrorKind::FileTooLarge.into());
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut output = Bounded(Vec::new());
    let records: Vec<_> = picks.iter().map(Export).collect();
    serde_json::to_writer(&mut output, &records).map_err(|failure| {
        if failure.io_error_kind() == Some(std::io::ErrorKind::FileTooLarge) {
            CitationError::OutputTooLarge
        } else {
            CitationError::InvalidRichText
        }
    })?;
    String::from_utf8(output.0).map_err(|_| CitationError::InvalidRichText)
}
