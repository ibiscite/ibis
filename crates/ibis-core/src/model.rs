//! Source-neutral reference model.
//!
//! A [`Reference`] keeps its original source identity and raw text next to
//! normalized fields. Meaning is preserved before anything is simplified:
//! titles keep protected capitalization and italics as CSL rich text, author
//! lists remember truncation ("and others"), and dates keep ranges and
//! uncertainty. Plain text for search and display is derived from these.
//! Identity is always the source plus the native key, never a citation key
//! alone (keys can repeat across libraries).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// Stable identity of a record: which source it came from and its key there.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RecordId {
    /// Source identifier, e.g. a canonical file path or `zotero:group:123`.
    pub source: String,
    /// Key of the record inside that source (BibTeX key, Zotero item key, ...).
    pub native_key: String,
}

/// A name, split the way CSL expects. Organizations use `literal`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Name {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub given: Option<String>,
    /// Particle that is dropped when sorting, e.g. BibTeX's "van der".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dropping_particle: Option<String>,
    /// Particle that stays with the family name, e.g. "de" in "de Gaulle" for some styles.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub non_dropping_particle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    /// A name that must not be split, such as an organization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub literal: Option<String>,
}

/// Marker for a truncated author list ("and others" in BibTeX).
pub const OTHERS: &str = "others";

impl Name {
    /// A literal name.
    pub fn literal(text: &str) -> Self {
        Name {
            literal: Some(text.to_string()),
            ..Name::default()
        }
    }

    /// True for the "and others" marker of a truncated author list.
    pub fn is_others(&self) -> bool {
        self.literal.as_deref() == Some(OTHERS)
    }

    /// Name in natural reading order: "given particle family suffix", family
    /// name first with no space for Chinese, Japanese and Korean names, and
    /// "et al." for a truncated list.
    pub fn display(&self) -> String {
        if self.is_others() {
            return "et al.".to_string();
        }
        if let Some(literal) = &self.literal {
            return literal.clone();
        }
        let is_cjk = |s: &Option<String>| s.as_deref().is_some_and(|s| s.chars().any(is_cjk_char));
        if is_cjk(&self.family) && (self.given.is_none() || is_cjk(&self.given)) {
            return [&self.family, &self.given]
                .into_iter()
                .flatten()
                .map(String::as_str)
                .collect();
        }
        [
            &self.given,
            &self.dropping_particle,
            &self.non_dropping_particle,
            &self.family,
            &self.suffix,
        ]
        .into_iter()
        .flatten()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" ")
    }

    fn to_csl(&self) -> Value {
        let mut m = Map::new();
        let fields = [
            ("family", &self.family),
            ("given", &self.given),
            ("dropping-particle", &self.dropping_particle),
            ("non-dropping-particle", &self.non_dropping_particle),
            ("suffix", &self.suffix),
            ("literal", &self.literal),
        ];
        for (key, value) in fields {
            if let Some(v) = value {
                m.insert(key.into(), json!(v));
            }
        }
        Value::Object(m)
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

/// A calendar date with optional month and day (1-based).
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

/// A publication date: a single date or a range, possibly approximate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issued {
    pub start: PartialDate,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<PartialDate>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub circa: bool,
}

impl Issued {
    pub fn year(year: i32) -> Self {
        Issued {
            start: PartialDate::year(year),
            end: None,
            circa: false,
        }
    }

    fn to_csl(self) -> Value {
        let mut parts = vec![self.start.date_parts()];
        if let Some(end) = self.end {
            parts.push(end.date_parts());
        }
        let mut m = Map::new();
        m.insert("date-parts".into(), Value::Array(parts));
        if self.circa {
            m.insert("circa".into(), json!(true));
        }
        Value::Object(m)
    }
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
    /// Title as CSL rich text: sentence case for English, with protected
    /// words in `<span class="nocase">` and italics in `<i>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<Name>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub editors: Vec<Name>,
    /// Further CSL name variables (`translator`, `container-author`,
    /// `compiler`, `director`, `contributor`, ...), keyed by CSL name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub other_names: BTreeMap<String, Vec<Name>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issued: Option<Issued>,
    /// Further CSL date variables (`accessed`, `original-date`, `event-date`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dates: BTreeMap<String, Issued>,
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
    /// BCP 47 language tag, e.g. `en-US`.
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

impl Reference {
    /// Publication year, if known.
    pub fn year(&self) -> Option<i32> {
        self.issued.map(|d| d.start.year)
    }

    /// Title without markup, for search and display.
    pub fn title_plain(&self) -> Option<String> {
        self.title.as_deref().map(strip_markup)
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
        if !self.authors.is_empty() {
            item.insert("author".into(), self.authors.iter().map(Name::to_csl).collect());
        }
        if !self.editors.is_empty() {
            item.insert("editor".into(), self.editors.iter().map(Name::to_csl).collect());
        }
        for (role, names) in &self.other_names {
            if !names.is_empty() {
                item.insert(role.clone(), names.iter().map(Name::to_csl).collect());
            }
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
        if let Some(issued) = self.issued {
            item.insert("issued".into(), issued.to_csl());
        }
        for (name, date) in &self.dates {
            item.insert(name.clone(), date.to_csl());
        }
        if !self.keywords.is_empty() {
            item.insert("keyword".into(), json!(self.keywords.join(", ")));
        }
        for (name, value) in &self.variables {
            item.entry(name.clone()).or_insert_with(|| json!(value));
        }
        Value::Object(item)
    }
}

/// Remove CSL rich-text tags and decode the entities used to escape text.
pub fn strip_markup(rich: &str) -> String {
    let mut out = String::with_capacity(rich.len());
    let mut in_tag = false;
    for c in rich.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

/// Category of a problem found while loading a source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WarningKind {
    /// The text could not be parsed as a record; the record was skipped.
    Syntax,
    /// A second record used a key that was already taken; it was skipped.
    DuplicateKey,
    /// The record was loaded, but part of it could not be interpreted.
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
