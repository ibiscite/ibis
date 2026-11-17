//! Citation keys for new `.bib` entries: the first author's family name, the
//! year and the first word of the title that is not a short function word,
//! as `taylor2026signals`. Letters are folded to ASCII and lowercased; parts
//! a reference lacks are left out. A key a file already uses, in any letter
//! case, gets the first free suffix `a` to `z`, then `aa` to `zz`.
use ibis_core::citable::CitableFields;
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

/// Letters kept of the name and of the title word.
const PART: usize = 20;

const SHORT_WORDS: [&str; 32] = [
    "a", "an", "the", "and", "or", "but", "nor", "of", "on", "in", "into", "at", "to", "for", "from", "by", "with",
    "as", "is", "are", "was", "be", "it", "its", "this", "that", "der", "die", "das", "le", "la", "les",
];

/// The key a new entry with `fields` takes, never one `taken` says is used.
/// `None` only when every suffix is taken.
pub fn citation_key(fields: &CitableFields, taken: impl Fn(&str) -> bool) -> Option<String> {
    let base = base(fields);
    if !taken(&base) {
        return Some(base);
    }
    let base = base.as_str();
    let letters = || (b'a'..=b'z').map(char::from);
    letters()
        .map(|letter| format!("{base}{letter}"))
        .chain(letters().flat_map(|first| letters().map(move |second| format!("{base}{first}{second}"))))
        .find(|key| !taken(key))
}

fn base(fields: &CitableFields) -> String {
    let name = ["author", "editor", "translator"]
        .iter()
        .find_map(|role| fields.names(role).first())
        .and_then(|name| {
            let name = serde_json::to_value(name).ok()?;
            name.get("family")
                .or_else(|| name.get("literal"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .map(|family| folded(&family))
        .unwrap_or_default();
    let plain = fields.plain();
    let year = plain
        .get("issued")
        .and_then(|issued| {
            issued
                .pointer("/date-parts/0/0")
                .and_then(Value::as_i64)
                .map(|year| year.to_string())
                .or_else(|| {
                    let text = issued.get("literal").or_else(|| issued.get("raw"))?.as_str()?;
                    ibis_core::citable::literal_year(text).map(|(year, _)| year.to_string())
                })
        })
        .filter(|year| year.len() == 4)
        .unwrap_or_default();
    let word = plain
        .get("title")
        .and_then(Value::as_str)
        .and_then(|title| {
            title
                .split(|c: char| !c.is_alphanumeric())
                .map(folded)
                .find(|word| !word.is_empty() && !SHORT_WORDS.contains(&word.as_str()))
        })
        .unwrap_or_default();
    let key = format!("{name}{year}{word}");
    if key.is_empty() { "ref".to_owned() } else { key }
}

/// Lowercase ASCII letters and digits only, with accents and ligatures folded.
fn folded(text: &str) -> String {
    let mut out = String::new();
    for c in text.nfkd().flat_map(char::to_lowercase) {
        match c {
            'a'..='z' | '0'..='9' => out.push(c),
            'ß' => out.push_str("ss"),
            'æ' => out.push_str("ae"),
            'œ' => out.push_str("oe"),
            'ø' => out.push('o'),
            'ł' => out.push('l'),
            'đ' | 'ð' => out.push('d'),
            'þ' => out.push_str("th"),
            'ı' => out.push('i'),
            _ => {}
        }
        if out.len() >= PART {
            break;
        }
    }
    out
}
