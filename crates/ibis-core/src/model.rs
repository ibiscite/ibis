//! Source-neutral names and dates shared by sources and search.
//!
//! Names are split the way CSL expects, and author lists remember truncation
//! ("and others"). Dates keep ranges and uncertainty. Plain text for search
//! and display is derived from these.

/// A name, split the way CSL expects. Organizations use `literal`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Name {
    pub family: Option<String>,
    pub given: Option<String>,
    /// Particle that is dropped when sorting, e.g. BibTeX's "van der".
    pub dropping_particle: Option<String>,
    /// Particle that stays with the family name, e.g. "de" in "de Gaulle" for some styles.
    pub non_dropping_particle: Option<String>,
    pub suffix: Option<String>,
    /// A name that must not be split, such as an organization.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartialDate {
    pub year: i32,
    pub month: Option<u8>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Issued {
    pub start: PartialDate,
    pub end: Option<PartialDate>,
    pub circa: bool,
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
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
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
        assert_eq!(strip_markup("&unknown; & <"), "&unknown; & ");
        assert_eq!(strip_markup(""), "");
    }
}
