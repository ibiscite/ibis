//! What a PDF says about itself (decision 0060): its Info dictionary, its XMP
//! packet and the DOI on its first page, read in the extractor's child and
//! checked here before anything uses them.
//!
//! The child reports what the file states, field by field, and decides
//! nothing. This side bounds every value, drops what is known to be noise
//! (a title that is a file name or a word processor's caption, an author that
//! is one word or a producer) and chooses one DOI: the XMP's, then the Info
//! dictionary's, then the only distinct DOI on the first page. Nothing is
//! filled in from elsewhere: a detail the file does not state stays unknown.

use super::{Refusal, MAX_PAGES};
use ibis_core::contract::Fingerprint;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

/// The most characters kept of one value; a longer one is not metadata.
pub const MAX_VALUE_CHARS: usize = 1024;
/// The most creators or identifiers one block may list.
pub const MAX_LIST: usize = 100;
/// The most pages of text the child sends, and the most bytes of each.
pub const DETAIL_PAGES: usize = 2;
pub const MAX_PAGE_TEXT_BYTES: usize = 64 * 1024;
/// The most authors kept, as a reference manager would show them.
const MAX_AUTHORS: usize = 50;

/// One metadata block as the file states it: the Info dictionary or the XMP
/// packet. Every value is the file's own text, unchecked.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stated {
    pub title: Option<String>,
    /// Each creator as written. The Info dictionary has one string for all.
    pub creators: Vec<String>,
    pub publisher: Option<String>,
    /// A DOI the block names as such (`prism:doi`, `pdfx:doi`,
    /// `crossmark:DOI`, or an Info `/doi`).
    pub doi: Option<String>,
    /// Other identifiers (`dc:identifier`), any scheme.
    pub identifiers: Vec<String>,
    /// The journal or book it appeared in (`prism:publicationName`).
    pub container: Option<String>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub first_page: Option<String>,
    pub last_page: Option<String>,
    /// The publication date (`prism:coverDate` or `prism:publicationDate`),
    /// never the file's own dates.
    pub date: Option<String>,
}

/// The child's answer to a details request.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Said {
    pub pages: u32,
    pub info: Stated,
    pub xmp: Stated,
    /// The text of the first pages, at most [`DETAIL_PAGES`].
    pub text: Vec<String>,
}
impl std::fmt::Debug for Said {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Said {{ pages: {}, private }}", self.pages)
    }
}

/// What a PDF says about itself, checked: each value in NFC, on one line,
/// at most [`MAX_VALUE_CHARS`], and absent when the file states nothing
/// usable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfDetails {
    /// Check of the verified answer and, for a file read, its bytes.
    pub revision: Fingerprint,
    pub pages: u32,
    pub title: Option<String>,
    /// Each author as the file writes the name.
    pub authors: Vec<String>,
    pub container: Option<String>,
    pub publisher: Option<String>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    /// `first` or `first-last`.
    pub page: Option<String>,
    /// `YYYY`, `YYYY-MM` or `YYYY-MM-DD`.
    pub issued: Option<String>,
    /// Canonical (lowercase, no resolver).
    pub doi: Option<String>,
}

/// The child's answer once it is within every bound, reduced to what the
/// file states usably.
pub fn checked_details(said: Said) -> Result<PdfDetails, Refusal> {
    if said.pages == 0 || said.pages > MAX_PAGES || said.text.len() > DETAIL_PAGES {
        return Err(Refusal::Damaged);
    }
    if said.text.iter().any(|page| page.len() > MAX_PAGE_TEXT_BYTES) {
        return Err(Refusal::TooLarge);
    }
    for block in [&said.info, &said.xmp] {
        if block.creators.len() > MAX_LIST || block.identifiers.len() > MAX_LIST {
            return Err(Refusal::TooLarge);
        }
    }
    let revision = Fingerprint::of_bytes(&serde_json::to_vec(&said).map_err(|_| Refusal::Damaged)?);
    let (info, xmp) = (&said.info, &said.xmp);
    let title = [&xmp.title, &info.title]
        .into_iter()
        .find_map(|title| title.as_deref().and_then(value).filter(|title| !noise_title(title)));
    let mut authors: Vec<String> = xmp.creators.iter().filter_map(|name| author(name)).collect();
    if authors.is_empty() {
        authors = info.creators.iter().flat_map(|names| split_names(names)).filter_map(author).collect();
    }
    authors.dedup();
    authors.truncate(MAX_AUTHORS);
    let first = |pick: fn(&Stated) -> &Option<String>| {
        [xmp, info].into_iter().find_map(|block| pick(block).as_deref().and_then(value))
    };
    let page = match (first(|b| &b.first_page), first(|b| &b.last_page)) {
        (Some(first), Some(last)) if short(&first) && short(&last) && first != last => Some(format!("{first}-{last}")),
        (Some(first), _) if short(&first) => Some(first),
        _ => None,
    };
    Ok(PdfDetails {
        revision,
        pages: said.pages,
        title,
        authors,
        container: first(|b| &b.container).filter(|name| !noise_title(name)),
        publisher: first(|b| &b.publisher),
        volume: first(|b| &b.volume).filter(|value| short(value)),
        issue: first(|b| &b.issue).filter(|value| short(value)),
        page,
        issued: first(|b| &b.date).as_deref().and_then(date),
        doi: doi(&said),
    })
}

/// A stated value as text Ibis may keep: NFC, control characters and runs of
/// white space made one space, trimmed, and within [`MAX_VALUE_CHARS`].
fn value(text: &str) -> Option<String> {
    let text: String = text.nfc().collect();
    let mut kept = String::with_capacity(text.len());
    for word in text.split(|c: char| c.is_whitespace() || c.is_control()) {
        if word.is_empty() {
            continue;
        }
        if !kept.is_empty() {
            kept.push(' ');
        }
        kept.push_str(word);
    }
    let count = kept.chars().count();
    // A value made only of marks, or replaced characters, says nothing.
    let readable = kept.chars().any(char::is_alphanumeric) && !kept.contains('\u{fffd}');
    (readable && count <= MAX_VALUE_CHARS).then_some(kept)
}

/// Volume, issue and page values are short.
fn short(value: &str) -> bool {
    value.chars().count() <= 32
}

/// Titles that name a file or a template rather than a work.
fn noise_title(title: &str) -> bool {
    let lower = title.to_lowercase();
    let letters = title.chars().filter(|c| c.is_alphabetic()).count();
    letters < 4
        || ["microsoft word -", "microsoft powerpoint -", "untitled", "title", "no title", "document", "slide 1"]
            .iter()
            .any(|noise| lower == *noise || noise.ends_with('-') && lower.starts_with(noise))
        || file_name(&lower)
}

/// `report.pdf`, `draft_v3.docx`, `C:\…` or `/Users/…`: a file, not a title.
fn file_name(lower: &str) -> bool {
    const EXTENSIONS: [&str; 12] = [
        ".pdf", ".doc", ".docx", ".odt", ".rtf", ".tex", ".dvi", ".ps", ".indd", ".qxd", ".txt", ".md",
    ];
    EXTENSIONS.iter().any(|extension| lower.ends_with(extension))
        || lower.contains(":\\")
        || lower.starts_with('/')
        || lower.starts_with("\\\\")
}

/// Software and publishers that write themselves into the author field.
fn producer(name: &str) -> bool {
    const PRODUCERS: [&str; 11] = [
        "adobe", "microsoft", "acrobat", "latex", "pdftex", "tex", "ghostscript", "administrator", "admin", "user",
        "owner",
    ];
    let lower = name.to_lowercase();
    PRODUCERS.iter().any(|producer| lower == *producer || lower.starts_with(&format!("{producer} ")))
}

/// The Info dictionary's one author string, split where it lists several:
/// on `;`, or on ` and ` and `&`, never on a comma, which may sit inside one
/// `Family, Given` name.
fn split_names(names: &str) -> Vec<&str> {
    if names.contains(';') {
        return names.split(';').collect();
    }
    names.split(" and ").flat_map(|part| part.split(" & ")).collect()
}

/// One stated author, when it reads as a person's or body's name: two words
/// or more, or `Family, Given`, and not a producer.
fn author(name: &str) -> Option<String> {
    let name = value(name)?;
    let words = name.split(' ').count();
    let named = words >= 2 || name.contains(',');
    (named && name.chars().count() <= 200 && !producer(&name) && !file_name(&name.to_lowercase())).then_some(name)
}

/// `YYYY`, `YYYY-MM` or `YYYY-MM-DD` from the start of an XMP date, with a
/// plausible year and month and day.
fn date(text: &str) -> Option<String> {
    let text = text.trim();
    let parts: Vec<&str> = text.get(..text.len().min(10))?.split('-').collect();
    let number = |part: &&str, digits: usize| (part.len() == digits && part.bytes().all(|b| b.is_ascii_digit())).then(|| part.parse::<u32>().ok()).flatten();
    let year = number(parts.first()?, 4).filter(|year| (1450..=2200).contains(year))?;
    let month = parts.get(1).and_then(|part| number(part, 2)).filter(|month| (1..=12).contains(month));
    let day = parts.get(2).and_then(|part| number(part, 2)).filter(|day| (1..=31).contains(day));
    Some(match (month, day) {
        (Some(month), Some(day)) => format!("{year:04}-{month:02}-{day:02}"),
        (Some(month), None) => format!("{year:04}-{month:02}"),
        _ => format!("{year:04}"),
    })
}

/// The file's DOI: one the XMP names, then the Info dictionary's, then a DOI
/// among either's identifiers, then the only distinct DOI on the first page.
/// Several distinct DOIs on that page give none: one is probably a reference.
fn doi(said: &Said) -> Option<String> {
    let named = [&said.xmp.doi, &said.info.doi]
        .into_iter()
        .flatten()
        .chain(said.xmp.identifiers.iter())
        .chain(said.info.identifiers.iter())
        .find_map(|value| identifier(value));
    named.or_else(|| {
        let page = said.text.first()?;
        let mut found = ibis_core::identifier::dois_in(page);
        found.sort();
        found.dedup();
        (found.len() == 1).then(|| found.remove(0))
    })
}

fn identifier(value: &str) -> Option<String> {
    match ibis_core::identifier::parse_query(value)? {
        ibis_core::identifier::Identifier::Doi(doi) => Some(doi),
        _ => None,
    }
}

#[cfg(test)]
#[path = "details_tests.rs"]
mod tests;
