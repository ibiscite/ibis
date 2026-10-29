//! Citation choices shared by settings and clients. Rendering uses `crate::syntax`.

use serde::{Deserialize, Serialize};

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
