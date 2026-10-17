//! Source-neutral reference model.
//!
//! A [`Reference`] keeps its original source identity and raw text next to the
//! normalized fields used for search and output. Identity is always the source
//! plus the native key, never a citation key alone (keys can repeat across
//! libraries).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// Stable identity of a record: which source it came from and its key there.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RecordId {
    /// Source identifier chosen by the caller, e.g. a file path label or `zotero:group:123`.
    pub source: String,
    /// Key of the record inside that source (BibTeX key, Zotero item key, ...).
    pub native_key: String,
}

/// A personal or organizational name, split the way CSL expects.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Name {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub given: Option<String>,
    /// Particle such as "van der" or "de".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub particle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
}

impl Name {
    /// Name in natural reading order: "given particle family suffix", or
    /// family name first with no space for Chinese, Japanese and Korean names.
    pub fn display(&self) -> String {
        let is_cjk = |s: &Option<String>| s.as_deref().is_some_and(|s| s.chars().any(is_cjk_char));
        if is_cjk(&self.family) && (self.given.is_none() || is_cjk(&self.given)) {
            return [&self.family, &self.given]
                .into_iter()
                .flatten()
                .map(String::as_str)
                .collect();
        }
        [&self.given, &self.particle, &self.family, &self.suffix]
            .into_iter()
            .flatten()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn is_cjk_char(c: char) -> bool {
    matches!(c,
        '\u{3040}'..='\u{30ff}'   // Hiragana, Katakana
        | '\u{3400}'..='\u{4dbf}' // CJK Extension A
        | '\u{4e00}'..='\u{9fff}' // CJK Unified Ideographs
        | '\u{ac00}'..='\u{d7af}' // Hangul syllables
        | '\u{f900}'..='\u{faff}' // CJK Compatibility Ideographs
        | '\u{20000}'..='\u{2fa1f}')
}

/// One bibliographic record from any source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reference {
    pub id: RecordId,
    /// Citation key used in LaTeX/Pandoc/Typst output, when the source has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citekey: Option<String>,
    /// CSL item type, e.g. `article-journal`.
    pub item_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<Name>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub editors: Vec<Name>,
    /// Publication date, as precise as the source states it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issued: Option<PartialDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abstract_text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Further CSL string variables (`volume`, `issue`, `page`, `publisher`,
    /// `edition`, `ISBN`, ...), keyed by their CSL variable name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, String>,
    /// Original source text of the record, kept verbatim so nothing is lost.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
}

/// A date with optional month and day (months and days are 1-based).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartialDate {
    pub year: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub month: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub day: Option<u8>,
}

impl PartialDate {
    pub fn year(year: i32) -> Self {
        PartialDate {
            year,
            month: None,
            day: None,
        }
    }

    fn date_parts(self) -> Value {
        match (self.month, self.day) {
            (Some(m), Some(d)) => json!([self.year, m, d]),
            (Some(m), None) => json!([self.year, m]),
            _ => json!([self.year]),
        }
    }
}

impl Reference {
    /// Publication year, if known.
    pub fn year(&self) -> Option<i32> {
        self.issued.map(|d| d.year)
    }

    /// Convert to a CSL-JSON item for citation processors.
    pub fn to_csl_json(&self) -> Value {
        let mut item = Map::new();
        let id = self.citekey.clone().unwrap_or_else(|| self.id.native_key.clone());
        item.insert("id".into(), json!(id));
        item.insert("type".into(), json!(self.item_type));
        if let Some(key) = &self.citekey {
            item.insert("citation-key".into(), json!(key));
        }
        let names = |list: &[Name]| -> Value {
            list.iter()
                .map(|n| {
                    let mut m = Map::new();
                    if let Some(v) = &n.family {
                        m.insert("family".into(), json!(v));
                    }
                    if let Some(v) = &n.given {
                        m.insert("given".into(), json!(v));
                    }
                    if let Some(v) = &n.particle {
                        m.insert("non-dropping-particle".into(), json!(v));
                    }
                    if let Some(v) = &n.suffix {
                        m.insert("suffix".into(), json!(v));
                    }
                    Value::Object(m)
                })
                .collect()
        };
        if !self.authors.is_empty() {
            item.insert("author".into(), names(&self.authors));
        }
        if !self.editors.is_empty() {
            item.insert("editor".into(), names(&self.editors));
        }
        let text_fields = [
            ("title", &self.title),
            ("container-title", &self.container_title),
            ("DOI", &self.doi),
            ("URL", &self.url),
            ("abstract", &self.abstract_text),
            ("language", &self.language),
        ];
        for (name, value) in text_fields {
            if let Some(v) = value {
                item.insert(name.into(), json!(v));
            }
        }
        if let Some(date) = self.issued {
            item.insert("issued".into(), json!({ "date-parts": [date.date_parts()] }));
        }
        for (name, value) in &self.variables {
            item.entry(name.clone()).or_insert_with(|| json!(value));
        }
        if !self.keywords.is_empty() {
            item.insert("keyword".into(), json!(self.keywords.join(", ")));
        }
        Value::Object(item)
    }
}

/// Category of a problem found while loading a source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WarningKind {
    /// The text could not be parsed as a record; the record was skipped.
    Syntax,
    /// A second record used a key that was already taken; it was skipped.
    DuplicateKey,
    /// The record was loaded, but one field could not be interpreted.
    Field,
}

/// A problem found while loading, reported instead of silently dropping data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadWarning {
    pub kind: WarningKind,
    /// 1-based line in the source text, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// Key of the affected record, when it could be determined.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub message: String,
}

/// Everything loaded from one source, plus what could not be loaded.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadResult {
    pub references: Vec<Reference>,
    pub warnings: Vec<LoadWarning>,
}
