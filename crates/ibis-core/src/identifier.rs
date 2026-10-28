//! Recognize DOIs, PubMed IDs, ISBNs and arXiv IDs, so pasting one finds the
//! record that carries it.
//!
//! Each identifier is reduced to one canonical form: DOIs lowercase without a
//! resolver prefix, ISBNs as 13 digits, arXiv IDs lowercase without a version,
//! PubMed IDs as digits. Recognition is strict (checksums, fixed shapes and
//! explicit prefixes), so ordinary search words are never taken for one.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A recognized identifier in canonical form.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Identifier {
    Doi(String),
    Isbn(String),
    Arxiv(String),
    Pmid(String),
}

/// The identifier a whole search query consists of, if it is one. Bare
/// numbers are not taken for PubMed IDs, because they are usually years or
/// pages; `pmid:` or a PubMed link is needed.
pub fn parse_query(query: &str) -> Option<Identifier> {
    let q = query.trim();
    // Identifiers are short; this also bounds the work done per keystroke.
    if q.len() > MAX_QUERY_LEN {
        return None;
    }
    let q = trim_wrapping(q);
    if q.is_empty() {
        return None;
    }
    doi(q)
        .map(Identifier::Doi)
        .or_else(|| arxiv(q).map(Identifier::Arxiv))
        .or_else(|| pmid(q).map(Identifier::Pmid))
        .or_else(|| isbn(q).map(Identifier::Isbn))
}

/// Longer pasted text is searched as words, never as an identifier.
const MAX_QUERY_LEN: usize = 512;

/// Drop sentence punctuation and unbalanced brackets around a pasted
/// identifier, keeping brackets that belong to it, as in `10.1000/abc(1)`.
fn trim_wrapping(mut q: &str) -> &str {
    loop {
        let before = q;
        q = q.trim_end_matches(['.', ',', ';', ':']).trim();
        for (open, close) in [('(', ')'), ('[', ']'), ('<', '>'), ('{', '}')] {
            let unbalanced = q.matches(open).count() != q.matches(close).count();
            if unbalanced && q.ends_with(close) {
                q = &q[..q.len() - close.len_utf8()];
            } else if unbalanced && q.starts_with(open) {
                q = &q[open.len_utf8()..];
            }
        }
        if q == before {
            return q;
        }
    }
}

fn with_equivalents(mut ids: Vec<Identifier>) -> Vec<Identifier> {
    // An arXiv ID and its arXiv DOI name the same work.
    let mut equivalents = Vec::new();
    for id in &ids {
        match id {
            Identifier::Arxiv(a) => equivalents.push(Identifier::Doi(format!("10.48550/arxiv.{a}"))),
            Identifier::Doi(d) => equivalents.extend(
                d.strip_prefix("10.48550/arxiv.")
                    .and_then(arxiv_id)
                    .map(Identifier::Arxiv),
            ),
            _ => {}
        }
    }
    ids.extend(equivalents);
    ids.sort();
    ids.dedup();
    ids
}

/// Normalised CSL fields and source-neutral search-only identifiers, without
/// requiring knowledge of a particular source.
pub fn of_record(record: &crate::contract::Record) -> Vec<Identifier> {
    let mut ids = record
        .identifiers
        .iter()
        .filter_map(|id| match id {
            Identifier::Doi(value) if value.len() <= MAX_QUERY_LEN => doi(value).map(Identifier::Doi),
            Identifier::Isbn(value) if value.len() <= MAX_QUERY_LEN => isbn(value).map(Identifier::Isbn),
            Identifier::Arxiv(value) if value.len() <= MAX_QUERY_LEN => arxiv(value).map(Identifier::Arxiv),
            Identifier::Pmid(value) if value.len() <= MAX_QUERY_LEN => {
                digits(value.trim()).or_else(|| pmid(value)).map(Identifier::Pmid)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    for (field, value) in &record.csl {
        let Some(value) = value.as_str() else {
            continue;
        };
        match field.as_str() {
            "DOI" => ids.extend(doi(value).map(Identifier::Doi)),
            "URL" => {
                ids.extend(doi(value).map(Identifier::Doi));
                ids.extend(arxiv(value).map(Identifier::Arxiv));
                ids.extend(pmid(value).map(Identifier::Pmid));
            }
            "ISBN" => {
                for part in value.split([',', ';']) {
                    match isbn(part) {
                        Some(found) => ids.push(Identifier::Isbn(found)),
                        None => ids.extend(part.split_whitespace().filter_map(isbn).map(Identifier::Isbn)),
                    }
                }
            }
            "PMID" => ids.extend(digits(value.trim()).map(Identifier::Pmid)),
            "number" | "archive_location" => ids.extend(
                value
                    .lines()
                    .filter_map(|line| arxiv_prefixed(line.trim()))
                    .map(Identifier::Arxiv),
            ),
            // Zotero keeps PMIDs in Extra, which reaches CSL as `note` lines.
            "note" => {
                for line in value.lines().map(str::trim) {
                    ids.extend(arxiv_prefixed(line).map(Identifier::Arxiv));
                    ids.extend(
                        strip_prefix_ci(line, "PMID:")
                            .and_then(|rest| digits(rest.trim()))
                            .map(Identifier::Pmid),
                    );
                }
            }
            _ => {}
        }
    }
    with_equivalents(ids)
}

fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| &text[prefix.len()..])
}

fn strip_link<'a>(text: &'a str, hosts: &[&str]) -> Option<&'a str> {
    let rest = ["https://", "http://", ""]
        .iter()
        .find_map(|s| strip_prefix_ci(text, s))?;
    let rest = strip_prefix_ci(rest, "www.").unwrap_or(rest);
    hosts.iter().find_map(|h| strip_prefix_ci(rest, h))
}

/// The path of a link, without a query string or fragment.
fn link_path(path: &str) -> &str {
    path.split(['?', '#']).next().unwrap_or(path).trim_end_matches('/')
}

/// Decode `%XX` escapes that a copied link may contain.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match (
            bytes[i],
            bytes.get(i + 1).copied().and_then(hex),
            bytes.get(i + 2).copied().and_then(hex),
        ) {
            (b'%', Some(high), Some(low)) => {
                out.push((high * 16 + low) as u8);
                i += 3;
            }
            (byte, _, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

fn doi(text: &str) -> Option<String> {
    let text = text.trim();
    let decoded;
    let body = match strip_link(text, &["doi.org/", "dx.doi.org/"]) {
        Some(path) => {
            decoded = percent_decode(path.split('#').next().unwrap_or(path));
            decoded.as_str()
        }
        None => strip_prefix_ci(text, "doi:")
            .or_else(|| strip_prefix_ci(text, "doi "))
            .unwrap_or(text),
    }
    .trim();
    let (prefix, suffix) = body.split_once('/')?;
    let registrant = prefix.strip_prefix("10.")?;
    let valid = registrant.len() >= 4
        && registrant.chars().all(|c| c.is_ascii_digit() || c == '.')
        && !suffix.is_empty()
        && !suffix.chars().any(char::is_whitespace);
    valid.then(|| body.to_lowercase())
}

fn arxiv(text: &str) -> Option<String> {
    let text = text.trim();
    if let Some(path) = strip_link(text, &["arxiv.org/abs/", "arxiv.org/pdf/", "arxiv.org/html/"]) {
        return arxiv_id(link_path(path).trim_end_matches(".pdf"));
    }
    // Both shapes are strict enough to recognize without a prefix.
    arxiv_prefixed(text).or_else(|| arxiv_id(text))
}

fn arxiv_prefixed(text: &str) -> Option<String> {
    arxiv_id(strip_prefix_ci(text, "arxiv:")?.trim())
}

/// New-style `2101.00001v2` or old-style `hep-th/9901001`, version dropped.
fn arxiv_id(text: &str) -> Option<String> {
    let id = text.to_lowercase();
    let id = match id.rsplit_once('v') {
        Some((base, version)) if !version.is_empty() && version.chars().all(|c| c.is_ascii_digit()) => base.to_string(),
        _ => id,
    };
    let new_style = id.split_once('.').is_some_and(|(month, number)| {
        month.len() == 4
            && month.chars().all(|c| c.is_ascii_digit())
            && (4..=5).contains(&number.len())
            && number.chars().all(|c| c.is_ascii_digit())
    });
    if new_style {
        return Some(id);
    }
    // Old style: `archive/YYMMNNN`; a subject class such as `math.AG/…` is
    // not part of the identifier.
    let (archive, number) = id.split_once('/')?;
    let archive = archive.split('.').next().unwrap_or(archive);
    let valid = !archive.is_empty()
        && archive.chars().all(|c| c.is_ascii_lowercase() || c == '-')
        && number.len() == 7
        && number.chars().all(|c| c.is_ascii_digit());
    valid.then(|| format!("{archive}/{number}"))
}

fn pmid(text: &str) -> Option<String> {
    let text = text.trim();
    let body = strip_link(text, &["pubmed.ncbi.nlm.nih.gov/", "ncbi.nlm.nih.gov/pubmed/"])
        .map(link_path)
        .or_else(|| strip_prefix_ci(text, "pmid:").map(str::trim))
        .or_else(|| strip_prefix_ci(text, "pmid ").map(str::trim))?;
    digits(body)
}

fn digits(text: &str) -> Option<String> {
    let valid = (1..=9).contains(&text.len()) && text.chars().all(|c| c.is_ascii_digit());
    valid
        .then(|| text.trim_start_matches('0').to_string())
        .filter(|d| !d.is_empty())
}

/// An ISBN-10 or ISBN-13 with a valid check digit, as 13 digits.
fn isbn(text: &str) -> Option<String> {
    let text = text.trim();
    let body = ["isbn-13:", "isbn-10:", "isbn-13", "isbn-10", "isbn:", "isbn"]
        .iter()
        .find_map(|prefix| strip_prefix_ci(text, prefix))
        .unwrap_or(text)
        .trim();
    if body
        .chars()
        .any(|c| !(c.is_ascii_digit() || matches!(c, '-' | ' ' | 'x' | 'X')))
    {
        return None;
    }
    let chars: Vec<char> = body.chars().filter(|c| !matches!(c, '-' | ' ')).collect();
    let value = |c: &char| {
        if matches!(c, 'x' | 'X') {
            10
        } else {
            c.to_digit(10).unwrap_or(0)
        }
    };
    match chars.len() {
        10 if chars[..9].iter().all(char::is_ascii_digit) => {
            let sum: u32 = chars.iter().enumerate().map(|(i, c)| (10 - i as u32) * value(c)).sum();
            if !sum.is_multiple_of(11) {
                return None;
            }
            let mut digits: String = "978".chars().chain(chars[..9].iter().copied()).collect();
            let check = (10 - isbn13_sum(&digits) % 10) % 10;
            digits.push(char::from_digit(check, 10)?);
            Some(digits)
        }
        13 if chars.iter().all(char::is_ascii_digit) && body.chars().filter(char::is_ascii_digit).count() == 13 => {
            let digits: String = chars.iter().collect();
            (matches!(&digits[..3], "978" | "979")
                && (isbn13_sum(&digits[..12]) + value(&chars[12])).is_multiple_of(10))
            .then_some(digits)
        }
        _ => None,
    }
}

fn isbn13_sum(digits: &str) -> u32 {
    digits
        .chars()
        .filter_map(|c| c.to_digit(10))
        .enumerate()
        .map(|(i, d)| if i % 2 == 0 { d } else { 3 * d })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_bracket_runs_are_rejected_quickly() {
        let started = std::time::Instant::now();
        assert_eq!(parse_query(&"(".repeat(100_000)), None);
        assert_eq!(parse_query(&format!("{}10.1000/x", "(".repeat(600))), None);
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        assert_eq!(
            parse_query(&format!("{}10.1000/x", "(".repeat(100))),
            Some(Identifier::Doi("10.1000/x".into()))
        );
    }

    #[test]
    fn queries_in_common_written_forms_are_recognized() {
        let doi = Some(Identifier::Doi("10.1000/xyz.123".into()));
        for q in [
            "10.1000/XYZ.123",
            "doi:10.1000/xyz.123",
            "https://doi.org/10.1000/xyz.123",
            "http://dx.doi.org/10.1000/xyz.123.",
            " DOI:10.1000/xyz.123 ",
        ] {
            assert_eq!(parse_query(q), doi, "{q}");
        }
        let arxiv = Some(Identifier::Arxiv("2101.00001".into()));
        for q in [
            "2101.00001",
            "arXiv:2101.00001v3",
            "https://arxiv.org/abs/2101.00001v2",
            "arxiv.org/pdf/2101.00001.pdf",
        ] {
            assert_eq!(parse_query(q), arxiv, "{q}");
        }
        let old_style = Some(Identifier::Arxiv("hep-th/9901001".into()));
        for q in [
            "arXiv:hep-th/9901001",
            "hep-th/9901001",
            "hep-th/9901001v2",
            "https://arxiv.org/abs/hep-th/9901001",
        ] {
            assert_eq!(parse_query(q), old_style, "{q}");
        }
        assert_eq!(
            parse_query("math.AG/0501001"),
            Some(Identifier::Arxiv("math/0501001".into()))
        );
        let pmid = Some(Identifier::Pmid("12345678".into()));
        for q in [
            "PMID:12345678",
            "pmid 12345678",
            "https://pubmed.ncbi.nlm.nih.gov/12345678/",
        ] {
            assert_eq!(parse_query(q), pmid, "{q}");
        }
        let isbn = Some(Identifier::Isbn("9780306406157".into()));
        for q in [
            "978-0-306-40615-7",
            "0-306-40615-2",
            "ISBN 0306406152",
            "isbn:978 0 306 40615 7",
        ] {
            assert_eq!(parse_query(q), isbn, "{q}");
        }
    }

    #[test]
    fn ordinary_search_words_are_not_identifiers() {
        for q in [
            "",
            "2021",
            "12345678",
            "smith2020",
            "sleep memory",
            "10.5",
            "10.1000/",
            "1.2",
            "978-0-306-40615-8",
            "0-306-40615-3",
            "9790306406157x",
            "2101.001",
            "pmid:",
            "pmid:abc",
            "doi:10.1000/a b",
            "and/or",
            "hep-th/99",
            "hep-th/99010012",
            "input/output",
        ] {
            assert_eq!(parse_query(q), None, "{q:?}");
        }
    }

    #[test]
    fn hostile_text_never_panics() {
        for q in [
            "ü",
            "doi:",
            "https://",
            "arxiv:",
            "isbnü",
            "10.ü/ü",
            "pmid:٣٣",
            "x".repeat(10_000).as_str(),
        ] {
            let _ = parse_query(q);
        }
    }
}
