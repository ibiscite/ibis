//! What reading a manuscript returns (decision 0060). The readers run only
//! in the document child; these are the answers the engine checks and uses.

use super::Locator;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The largest manuscript read, in bytes.
pub const MAX_TEXT_BYTES: usize = 16 * 1024 * 1024;

/// The longest quotation read before a citation, in characters.
pub const MAX_QUOTE_CHARS: usize = 1000;

/// How a manuscript writes its citations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TextKind {
    /// Markdown, Quarto and R Markdown: Pandoc's `[@key, p. 4]` and `@key`.
    Markdown,
    /// LaTeX with BibTeX, natbib or BibLaTeX commands.
    Latex,
    /// Typst's `@key` and `#cite(<key>)`.
    Typst,
}

impl TextKind {
    /// The kind a file's extension names.
    pub fn of(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        Some(match extension.as_str() {
            "md" | "markdown" | "qmd" | "rmd" => Self::Markdown,
            "tex" | "ltx" => Self::Latex,
            "typ" => Self::Typst,
            _ => return None,
        })
    }
}

/// Where something is, as a person finds it in an editor: line and
/// character, both from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Place {
    pub line: usize,
    pub column: usize,
}

/// One reference in a citation, with what the document wrote around it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Cite {
    pub key: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prefix: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locator: Option<Locator>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub suffix: String,
    /// `-@key`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub suppress_author: bool,
}

/// One citation, with its references in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Cited {
    pub place: Place,
    /// The author is part of the sentence (`@key`, `\textcite`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub narrative: bool,
    pub cites: Vec<Cite>,
    /// The words of a quotation that ends just before the citation, as the
    /// manuscript writes them without markup: `"…" [@key, p. 4]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
}

/// Why text that starts a citation is not one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ScanProblem {
    /// `[@key` with no closing bracket; the rest of its paragraph is unread.
    Unclosed,
    /// A part between semicolons, or an `@`, with no key.
    WithoutKey,
    /// Two keys without a semicolon between them.
    TwoKeys,
    /// A braced locator that is not one, such as `{about here}`.
    Locator,
    /// More text than one reading may spend; nothing after it was read.
    TooLong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
/// A citation that could not be read, where it starts.
pub struct ScanFinding {
    pub place: Place,
    pub kind: ScanProblem,
}

/// A manuscript's citations and problems, in document order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Manuscript {
    pub citations: Vec<Cited>,
    pub problems: Vec<ScanFinding>,
    /// `\nocite{*}`: every entry of the bibliography counts as cited.
    #[serde(default)]
    pub cites_everything: bool,
}

impl Manuscript {
    /// Whether the whole text was read: no limit or cancellation stopped it.
    pub fn complete(&self) -> bool {
        !self.problems.iter().any(|problem| problem.kind == ScanProblem::TooLong)
    }
}
