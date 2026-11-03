//! Source-neutral names, dates and plain-text helpers shared by sources and search.

use serde::{Deserialize, Serialize};

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
}

pub(crate) fn is_cjk_char(c: char) -> bool {
    matches!(c,
        '\u{1100}'..='\u{11ff}'   // Hangul Jamo (including canonically decomposed syllables)
        | '\u{3040}'..='\u{30ff}' // Hiragana, Katakana
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
}

/// Remove CSL rich-text tags and decode the entities used to escape text.
/// Markup is exactly what the formatter reads (`crates/ibis-csl/src/bridge.js`);
/// any other `<`, as in `p<0.05`, is text.
pub fn strip_markup(rich: &str) -> String {
    let mut out = String::with_capacity(rich.len());
    let mut rest = rich;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        match rich_text_tag_len(rest) {
            Some(length) => rest = &rest[length..],
            None => {
                out.push('<');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

/// How far a span's attributes are searched for its `>`, so hostile text stays linear.
const MAX_TAG_ATTRIBUTES: usize = 256;

/// The length of the rich-text tag `text` starts with, if it starts with one.
fn rich_text_tag_len(text: &str) -> Option<usize> {
    const PLAIN: [&str; 11] = [
        "<i>", "</i>", "<b>", "</b>", "<sc>", "</sc>", "<sub>", "</sub>", "<sup>", "</sup>", "</span>",
    ];
    if let Some(tag) = PLAIN.iter().find(|tag| text.starts_with(**tag)) {
        return Some(tag.len());
    }
    let after_name = text.strip_prefix("<span")?;
    let attribute = after_name.trim_start();
    if attribute.len() == after_name.len() {
        return None;
    }
    let after_value = attribute
        .strip_prefix("class=\"no")
        .and_then(|value| value.strip_prefix("case\"").or_else(|| value.strip_prefix("decor\"")))
        .or_else(|| {
            let value = attribute.strip_prefix("style=\"font-variant:")?.trim_start();
            let value = value.strip_prefix("small-caps")?;
            value.strip_prefix(';').unwrap_or(value).strip_prefix('"')
        })?;
    let end = after_value
        .bytes()
        .take(MAX_TAG_ATTRIBUTES)
        .position(|byte| byte == b'>')?;
    Some(text.len() - after_value.len() + end + 1)
}

#[cfg(test)]
mod tests {
    use super::strip_markup;

    #[test]
    fn strip_markup_removes_tags_and_decodes_entities() {
        assert_eq!(strip_markup("<i>Q</i> &amp; A"), "Q & A");
        assert_eq!(strip_markup("a &lt;b&gt; &quot;c&quot;"), "a <b> \"c\"");
        // Double-escaped text decodes exactly one level.
        assert_eq!(strip_markup("&amp;lt;x&amp;gt;"), "&lt;x&gt;");
        assert_eq!(strip_markup("&amp;amp;"), "&amp;");
        assert_eq!(strip_markup("&unknown; & <"), "&unknown; & <");
        assert_eq!(strip_markup(""), "");
    }

    #[test]
    fn only_tags_the_formatter_reads_are_markup() {
        assert_eq!(strip_markup("Effect of p<0.05 on tides"), "Effect of p<0.05 on tides");
        assert_eq!(strip_markup("a < b > c"), "a < b > c");
        for literal in [
            "<foo>bar</foo>",
            "Why A<B>C matters",
            "<span>x",
            "<span class=\"math\">x",
            "<I>x",
        ] {
            assert_eq!(strip_markup(literal), literal);
        }
        assert_eq!(
            strip_markup("<span class=\"nocase\">iPhone</span> and <sup>2</sup>H <sc>Ab</sc>"),
            "iPhone and 2H Ab"
        );
        for spelled in [
            "<span  class=\"nocase\">x</span>",
            "<span\tclass=\"nodecor\">x</span>",
            "<span class=\"nocase\" data-origin=\"invented\">x</span>",
            "<span style=\"font-variant:small-caps;\">x</span>",
            "<span style=\"font-variant: small-caps\">x</span>",
        ] {
            assert_eq!(strip_markup(spelled), "x", "{spelled}");
        }
        // A tag that never closes is text, even repeated many times.
        let unclosed = "<span class=\"nocase\"".repeat(10_000);
        assert_eq!(strip_markup(&unclosed), unclosed);
        // A long run of `<` stays linear and literal.
        assert_eq!(strip_markup(&"<".repeat(100_000)), "<".repeat(100_000));
    }
}
