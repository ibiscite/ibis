//! Citation-key outputs: Pandoc/Markdown, BibLaTeX, plain LaTeX, Typst.
//!
//! Formatted citations (IEEE, APA, ...) come from the CSL processor; this
//! module produces the key-based syntaxes that writers paste into source
//! documents. Each syntax encodes locators, prefixes and suppressed authors
//! differently, and each has its own escaping rules. When a syntax cannot
//! express something, the output says so in `warnings` instead of guessing.

use crate::syntax::escape::{
    is_range, latex_escape, normalize_dashes, pandoc_escape, pandoc_key, typst_markup_escape, typst_simple_label,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

mod input;
pub use input::{InputError, MAX_INPUT_BYTES, MAX_INPUT_ITEMS, read_items};

/// Kind of locator ("p. 12", "chap. 3").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "type-export", derive(specta::Type))]
#[serde(rename_all = "kebab-case")]
pub enum LocatorLabel {
    Page,
    Chapter,
    Section,
    Paragraph,
    Figure,
    Line,
    Note,
    Volume,
    Verse,
}

impl LocatorLabel {
    /// English abbreviation understood by Pandoc's citeproc and readable in LaTeX.
    fn abbreviation(self, plural: bool) -> &'static str {
        match (self, plural) {
            (LocatorLabel::Page, false) => "p.",
            (LocatorLabel::Page, true) => "pp.",
            (LocatorLabel::Chapter, _) => "chap.",
            (LocatorLabel::Section, _) => "sec.",
            (LocatorLabel::Paragraph, _) => "para.",
            (LocatorLabel::Figure, _) => "fig.",
            (LocatorLabel::Line, false) => "l.",
            (LocatorLabel::Line, true) => "ll.",
            (LocatorLabel::Note, _) => "n.",
            (LocatorLabel::Volume, _) => "vol.",
            (LocatorLabel::Verse, false) => "v.",
            (LocatorLabel::Verse, true) => "vv.",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "type-export", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct Locator {
    pub label: LocatorLabel,
    /// As typed by the user, e.g. "12", "24-26", "3, 7".
    pub value: String,
}

/// One cited reference inside a citation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CiteItem {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locator: Option<Locator>,
    /// Text before the reference, e.g. "see".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    /// Text after the locator, e.g. "emphasis added".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    /// Show only the year, for "Smith argues (2020)".
    #[serde(default)]
    pub suppress_author: bool,
}

/// Key-based output syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "type-export", derive(specta::Type))]
#[serde(rename_all = "kebab-case")]
pub enum KeyFormat {
    /// Pandoc / Quarto / R Markdown / Obsidian: `[@key, p. 12]`.
    Pandoc,
    /// BibLaTeX `\autocite` / `\autocites`.
    Autocite,
    /// BibLaTeX `\parencite` / `\parencites`.
    Parencite,
    /// BibLaTeX `\textcite` / `\textcites`.
    Textcite,
    /// Plain LaTeX / natbib-compatible `\cite`.
    Cite,
    /// Typst `@key[p. 12]`.
    Typst,
    /// Bare keys separated by commas.
    Keys,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rendered {
    pub text: String,
    /// Things the chosen syntax could not express exactly.
    pub warnings: Vec<String>,
}

/// Transient preview ownership, in input order. Literal shared syntax has no
/// owner; indices identify choices even when their citation keys repeat.
/// This metadata is not part of the serialized citation or clipboard output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationSegment {
    pub item: Option<usize>,
    pub text: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CiteError {
    #[error("no references selected")]
    Empty,
    #[error("citation key `{0}` cannot be used: it is empty or contains a space, comma, brace or control character")]
    InvalidKey(String),
    #[error("text contains a control character")]
    ControlCharacter,
    #[error("a locator must contain non-whitespace text")]
    EmptyLocator,
    #[error("citation key `{0}` contains characters that cannot be used in {1}; rename the key in the library")]
    KeyNotValidFor(String, &'static str),
}

/// Render a citation in the requested key syntax.
pub fn render(format: KeyFormat, items: &[CiteItem]) -> Result<Rendered, CiteError> {
    render_with_segments(format, items).map(|(rendered, _)| rendered)
}

/// Render once, retaining the per-reference pieces made by the syntax itself.
/// Concatenating their text exactly reproduces `Rendered::text`; callers must
/// never add markers to input keys or infer ownership from the resulting text.
pub fn render_with_segments(
    format: KeyFormat,
    items: &[CiteItem],
) -> Result<(Rendered, Vec<CitationSegment>), CiteError> {
    if items.is_empty() {
        return Err(CiteError::Empty);
    }
    let latex = matches!(
        format,
        KeyFormat::Autocite | KeyFormat::Parencite | KeyFormat::Textcite | KeyFormat::Cite
    );
    for item in items {
        validate_key(&item.key)?;
        if item
            .locator
            .as_ref()
            .is_some_and(|locator| locator.value.trim().is_empty())
        {
            return Err(CiteError::EmptyLocator);
        }
        // These characters change meaning inside a LaTeX argument (comment,
        // parameter, command, active character) and cannot be escaped in a key.
        if latex && item.key.contains(['%', '#', '\\', '~', '^', '$', '&']) {
            return Err(CiteError::KeyNotValidFor(item.key.clone(), "LaTeX"));
        }
        let texts = [
            item.prefix.as_deref(),
            item.suffix.as_deref(),
            item.locator.as_ref().map(|l| l.value.as_str()),
        ];
        if texts.into_iter().flatten().any(|t| t.chars().any(char::is_control)) {
            return Err(CiteError::ControlCharacter);
        }
    }
    let mut warnings = Vec::new();
    let segments = match format {
        KeyFormat::Pandoc => pandoc(items),
        KeyFormat::Autocite => biblatex("autocite", items, &mut warnings),
        KeyFormat::Parencite => biblatex("parencite", items, &mut warnings),
        KeyFormat::Textcite => biblatex("textcite", items, &mut warnings),
        KeyFormat::Cite => plain_cite(items, &mut warnings),
        KeyFormat::Typst => typst(items, &mut warnings),
        KeyFormat::Keys => {
            if items
                .iter()
                .any(|i| i.locator.is_some() || i.prefix.is_some() || i.suffix.is_some() || i.suppress_author)
            {
                warnings.push("bare keys cannot carry page numbers or notes; they were left out".to_string());
            }
            segments("", items.iter().map(|i| i.key.clone()), ", ", "")
        }
    };
    let text = segments.iter().map(|part| part.text.as_str()).collect();
    Ok((Rendered { text, warnings }, segments))
}

fn segments(
    prefix: &str,
    parts: impl IntoIterator<Item = String>,
    separator: &str,
    suffix: &str,
) -> Vec<CitationSegment> {
    let mut out = Vec::new();
    if !prefix.is_empty() {
        out.push(CitationSegment {
            item: None,
            text: prefix.into(),
        });
    }
    for (index, text) in parts.into_iter().enumerate() {
        if index > 0 && !separator.is_empty() {
            out.push(CitationSegment {
                item: None,
                text: separator.into(),
            });
        }
        out.push(CitationSegment {
            item: Some(index),
            text,
        });
    }
    if !suffix.is_empty() {
        out.push(CitationSegment {
            item: None,
            text: suffix.into(),
        });
    }
    out
}

fn validate_key(key: &str) -> Result<(), CiteError> {
    let bad = key.is_empty()
        || key
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, ',' | '{' | '}'));
    if bad {
        Err(CiteError::InvalidKey(key.to_string()))
    } else {
        Ok(())
    }
}

/// Locator text with an abbreviation, using `dash` between range ends.
fn locator_text(locator: &Locator, dash: &str, space: &str) -> String {
    let value = normalize_dashes(locator.value.trim(), dash);
    format!("{}{space}{value}", locator.label.abbreviation(is_range(&locator.value)))
}

// ---------- Pandoc ----------

fn pandoc(items: &[CiteItem]) -> Vec<CitationSegment> {
    let parts: Vec<String> = items
        .iter()
        .map(|item| {
            let mut s = String::new();
            if let Some(prefix) = &item.prefix {
                s.push_str(&pandoc_escape(prefix));
                s.push(' ');
            }
            if item.suppress_author {
                s.push('-');
            }
            s.push('@');
            s.push_str(&pandoc_key(&item.key));
            if let Some(locator) = &item.locator {
                s.push_str(", ");
                s.push_str(&pandoc_escape(&locator_text(locator, "-", " ")));
            }
            if let Some(suffix) = &item.suffix {
                s.push_str(", ");
                s.push_str(&pandoc_escape(suffix));
            }
            s
        })
        .collect();
    segments("[", parts, "; ", "]")
}

// ---------- LaTeX ----------

/// "chap.~3" style locator with the value escaped and ranges as `--`.
fn latex_locator(locator: &Locator) -> String {
    let value = latex_escape(&normalize_dashes(locator.value.trim(), "--"));
    format!("{}~{value}", locator.label.abbreviation(is_range(&locator.value)))
}

/// BibLaTeX postnote: bare page numbers (BibLaTeX adds "p."/"pp." itself),
/// other locators spelled out.
fn biblatex_postnote(item: &CiteItem) -> Option<String> {
    let locator = item.locator.as_ref().map(|l| match l.label {
        LocatorLabel::Page => latex_escape(&normalize_dashes(l.value.trim(), "--")),
        _ => latex_locator(l),
    });
    match (locator, item.suffix.as_deref().map(latex_escape)) {
        (Some(l), Some(s)) => Some(format!("{l}, {s}")),
        (Some(l), None) => Some(l),
        (None, Some(s)) => Some(s),
        (None, None) => None,
    }
}

fn biblatex_notes(item: &CiteItem) -> String {
    let pre = item.prefix.as_deref().map(latex_escape);
    match (pre, biblatex_postnote(item)) {
        (Some(pre), post) => format!("[{pre}][{}]", post.unwrap_or_default()),
        (None, Some(post)) => format!("[{post}]"),
        (None, None) => String::new(),
    }
}

fn biblatex(command: &str, items: &[CiteItem], warnings: &mut Vec<String>) -> Vec<CitationSegment> {
    if let [item] = items {
        // BibLaTeX's starred \autocite* and \parencite* omit the author;
        // \textcite has no such form because the author is the point of it.
        let star = match (item.suppress_author, command) {
            (true, "textcite") => {
                warnings.push("\\textcite always names the author; use \\parencite to show only the year".to_string());
                ""
            }
            (true, _) => "*",
            (false, _) => "",
        };
        return segments(
            "",
            [format!("\\{command}{star}{}{{{}}}", biblatex_notes(item), item.key)],
            "",
            "",
        );
    }
    if items.iter().any(|i| i.suppress_author) {
        warnings.push(format!(
            "\\{command}s cannot hide the author of one reference; author shown"
        ));
    }
    let parts = items
        .iter()
        .map(|i| format!("{}{{{}}}", biblatex_notes(i), i.key))
        .collect::<Vec<_>>();
    segments(&format!("\\{command}s"), parts, "", "")
}

fn plain_cite(items: &[CiteItem], warnings: &mut Vec<String>) -> Vec<CitationSegment> {
    if items.iter().any(|i| i.suppress_author) {
        warnings.push("plain \\cite has no author-suppressing form; author shown".to_string());
    }
    let annotated = items
        .iter()
        .any(|i| i.locator.is_some() || i.prefix.is_some() || i.suffix.is_some());
    if !annotated {
        return segments("\\cite{", items.iter().map(|i| i.key.clone()), ",", "}");
    }
    if items.len() > 1 {
        warnings.push("plain \\cite holds one note per command; one \\cite per reference was used".to_string());
    }
    let parts = items
        .iter()
        .map(|item| {
            let locator = item.locator.as_ref().map(latex_locator);
            let note = [
                item.prefix.as_deref().map(latex_escape),
                locator,
                item.suffix.as_deref().map(latex_escape),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(", ");
            if note.is_empty() {
                format!("\\cite{{{}}}", item.key)
            } else {
                format!("\\cite[{note}]{{{}}}", item.key)
            }
        })
        .collect::<Vec<_>>();
    segments("", parts, ", ", "")
}

// ---------- Typst ----------

fn typst(items: &[CiteItem], warnings: &mut Vec<String>) -> Vec<CitationSegment> {
    if items.iter().any(|i| i.prefix.is_some()) {
        warnings.push("Typst citations have no prefix; prefix placed before the citation as plain text".to_string());
    }
    let parts = items
        .iter()
        .map(|item| {
            let supplement = [
                item.locator.as_ref().map(|l| locator_text(l, "–", " ")),
                item.suffix.clone(),
            ]
            .into_iter()
            .flatten()
            .map(|s| typst_markup_escape(&s))
            .collect::<Vec<_>>()
            .join(", ");
            let prefix = item
                .prefix
                .as_deref()
                .map(|p| format!("{} ", typst_markup_escape(p)))
                .unwrap_or_default();
            let simple = typst_simple_label(&item.key);
            let body = if item.suppress_author || !simple {
                let label = if simple {
                    format!("<{}>", item.key)
                } else {
                    format!("label(\"{}\")", item.key.replace('\\', "\\\\").replace('"', "\\\""))
                };
                let mut args = vec![label];
                if !supplement.is_empty() {
                    args.push(format!("supplement: [{supplement}]"));
                }
                if item.suppress_author {
                    args.push("form: \"year\"".to_string());
                }
                format!("#cite({})", args.join(", "))
            } else if supplement.is_empty() {
                format!("@{}", item.key)
            } else {
                format!("@{}[{supplement}]", item.key)
            };
            format!("{prefix}{body}")
        })
        .collect::<Vec<_>>();
    segments("", parts, " ", "")
}
