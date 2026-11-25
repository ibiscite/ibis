//! What a PDF says about itself, for `ibis __extract-pdf --details`
//! (decision 0061): the Info dictionary, the XMP packet and the text of the
//! first pages, reported as the file states them. The parent bounds, checks
//! and chooses (`ibis_sources::pdf::checked_details`); this side only reads.
//!
//! XMP is XML: a document type, an entity or anything past the size, depth and
//! work bounds ends the XMP read, and the Info dictionary and page text are
//! still answered.

use crate::pdf_text::{load, page_text};
use ibis_engine::pdf::{DETAIL_PAGES, MAX_LIST, MAX_PAGE_TEXT_BYTES, MAX_VALUE_CHARS, Refusal, Said, Stated};
use lopdf::{Document, Object};
use quick_xml::{Reader, events::Event};
use std::collections::BTreeMap;

/// The largest XMP packet read, once inflated.
const MAX_XMP_BYTES: usize = 1024 * 1024;
const MAX_DEPTH: usize = 64;
const MAX_NAME_BYTES: usize = 256;
const MAX_ATTRIBUTES: usize = 128;
/// Bytes of one value kept here; the parent keeps far fewer characters.
const MAX_VALUE_BYTES: usize = 4 * MAX_VALUE_CHARS;

/// What the PDF in `bytes` says about itself.
pub fn details(bytes: &[u8]) -> Result<Said, Refusal> {
    let (document, pages) = load(bytes)?;
    let count = u32::try_from(pages.len()).map_err(|_| Refusal::TooLarge)?;
    let text = pages
        .iter()
        .take(DETAIL_PAGES)
        .map(|page| truncated(page_text(&document, *page), MAX_PAGE_TEXT_BYTES))
        .collect();
    Ok(Said {
        pages: count,
        info: info(&document),
        xmp: metadata_stream(&document)
            .map(|packet| xmp(&packet))
            .unwrap_or_default(),
        text,
    })
}

/// `text` cut to at most `limit` bytes, on a character boundary.
fn truncated(mut text: String, limit: usize) -> String {
    if text.len() > limit {
        let mut end = limit;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    text
}

/// The trailer's `/Info`: Title, Author and a `/doi` or `/DOI` some
/// publishers add.
fn info(document: &Document) -> Stated {
    let Some(info) = document
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|object| document.dereference(object).ok())
        .and_then(|(_, object)| object.as_dict().ok())
    else {
        return Stated::default();
    };
    let text = |key: &[u8]| {
        let object = document.dereference(info.get(key).ok()?).ok()?.1;
        lopdf::decode_text_string(object)
            .ok()
            .map(|value| truncated(value, MAX_VALUE_BYTES))
    };
    Stated {
        title: text(b"Title"),
        creators: text(b"Author").into_iter().collect(),
        doi: text(b"doi").or_else(|| text(b"DOI")),
        ..Stated::default()
    }
}

/// The catalog's `/Metadata` stream, inflated within [`MAX_XMP_BYTES`].
fn metadata_stream(document: &Document) -> Option<String> {
    let catalog = document.catalog().ok()?;
    let (_, object) = document.dereference(catalog.get(b"Metadata").ok()?).ok()?;
    let Object::Stream(stream) = object else {
        return None;
    };
    let bytes = stream.get_plain_content_with_limit(MAX_XMP_BYTES).ok()?;
    String::from_utf8(bytes).ok()
}

/// The properties read, by their usual prefix and their namespace.
const PROPERTIES: [(&str, &str, &str); 14] = [
    ("dc", "title", "title"),
    ("dc", "creator", "creator"),
    ("dc", "publisher", "publisher"),
    ("dc", "identifier", "identifier"),
    ("prism", "doi", "doi"),
    ("pdfx", "doi", "doi"),
    ("crossmark", "DOI", "doi"),
    ("prism", "publicationName", "container"),
    ("prism", "volume", "volume"),
    ("prism", "number", "issue"),
    ("prism", "startingPage", "first-page"),
    ("prism", "endingPage", "last-page"),
    ("prism", "coverDate", "date"),
    ("prism", "publicationDate", "date"),
];

/// The usual prefix of a namespace this reader knows.
fn known_namespace(uri: &str) -> Option<&'static str> {
    match uri {
        "http://purl.org/dc/elements/1.1/" => Some("dc"),
        "http://ns.adobe.com/pdfx/1.3/" => Some("pdfx"),
        "http://crossref.org/crossmark/1.0/" => Some("crossmark"),
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#" => Some("rdf"),
        uri if uri.starts_with("http://prismstandard.org/namespaces/") => Some("prism"),
        _ => None,
    }
}

/// The XMP packet's properties, by the names in [`PROPERTIES`]. Prefixes are
/// read through the packet's own `xmlns` declarations, falling back to the
/// usual prefix when it declares none.
fn xmp(packet: &str) -> Stated {
    let mut found: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    // A damaged packet keeps what was read before the damage.
    let _ = read_xmp(packet, &mut found);
    let one = |name: &str| found.get(name).and_then(|values| values.first().cloned());
    let many = |name: &str| found.get(name).cloned().unwrap_or_default();
    Stated {
        title: one("title"),
        creators: many("creator"),
        publisher: one("publisher"),
        doi: one("doi"),
        identifiers: many("identifier"),
        container: one("container"),
        volume: one("volume"),
        issue: one("issue"),
        first_page: one("first-page"),
        last_page: one("last-page"),
        date: one("date"),
    }
}

struct Names {
    /// Prefix to the usual prefix of its namespace, as declared so far.
    prefixes: BTreeMap<String, &'static str>,
}
impl Names {
    /// `prefix:local` with its prefix made the usual one, when known.
    fn usual(&self, name: &str) -> Option<(&'static str, String)> {
        let (prefix, local) = name.split_once(':')?;
        let usual = self.prefixes.get(prefix).copied().or_else(|| {
            ["dc", "prism", "pdfx", "crossmark", "rdf"]
                .into_iter()
                .find(|usual| *usual == prefix)
        })?;
        Some((usual, local.to_owned()))
    }
    fn property(&self, name: &str) -> Option<&'static str> {
        let (prefix, local) = self.usual(name)?;
        PROPERTIES
            .iter()
            .find(|(p, l, _)| *p == prefix && *l == local)
            .map(|(_, _, property)| *property)
    }
    fn is_rdf(&self, name: &str, local: &str) -> bool {
        self.usual(name)
            .is_some_and(|(prefix, l)| prefix == "rdf" && l == local)
    }
}

/// One property being read: its name, its `rdf:li` values and the text
/// directly inside it.
struct Open {
    property: &'static str,
    depth: usize,
    values: Vec<String>,
    /// The `x-default` alternative of a language alternative, when given.
    default: Option<String>,
    text: String,
    item: Option<(String, bool)>,
}

#[derive(Debug)]
struct Stop;

fn read_xmp(packet: &str, found: &mut BTreeMap<&'static str, Vec<String>>) -> Result<(), Stop> {
    let mut reader = Reader::from_str(packet);
    reader.config_mut().expand_empty_elements = true;
    let mut names = Names {
        prefixes: BTreeMap::new(),
    };
    let mut depth = 0usize;
    let mut open: Option<Open> = None;
    let mut work = 0usize;
    loop {
        let before = usize::try_from(reader.buffer_position()).map_err(|_| Stop)?;
        let event = reader.read_event().map_err(|_| Stop)?;
        let after = usize::try_from(reader.buffer_position()).map_err(|_| Stop)?;
        work = work.saturating_add(after.saturating_sub(before)).saturating_add(64);
        if work > 4 * MAX_XMP_BYTES {
            return Err(Stop);
        }
        match event {
            Event::Start(tag) => {
                let qname = tag.name();
                let name: &str = qname.as_ref();
                if name.len() > MAX_NAME_BYTES {
                    return Err(Stop);
                }
                depth += 1;
                if depth > MAX_DEPTH {
                    return Err(Stop);
                }
                let mut attributes = Vec::new();
                for (index, attribute) in tag.attributes().enumerate() {
                    let attribute = attribute.map_err(|_| Stop)?;
                    let key: &str = attribute.key.as_ref();
                    if index >= MAX_ATTRIBUTES || key.len() > MAX_NAME_BYTES {
                        return Err(Stop);
                    }
                    let value = quick_xml::escape::unescape(&attribute.value).map_err(|_| Stop)?;
                    if let Some(prefix) = key.strip_prefix("xmlns:")
                        && let Some(usual) = known_namespace(&value)
                    {
                        names.prefixes.insert(prefix.to_owned(), usual);
                    }
                    attributes.push((key.to_owned(), value.into_owned()));
                }
                if let Some(current) = &mut open {
                    if names.is_rdf(name, "li") {
                        let default = attributes
                            .iter()
                            .any(|(key, value)| key == "xml:lang" && value == "x-default");
                        current.item = Some((String::new(), default));
                    }
                    continue;
                }
                if names.is_rdf(name, "Description") {
                    // Properties written as attributes of the description.
                    for (key, value) in &attributes {
                        if let Some(property) = names.property(key) {
                            keep(found, property, value);
                        }
                    }
                } else if let Some(property) = names.property(name) {
                    open = Some(Open {
                        property,
                        depth,
                        values: Vec::new(),
                        default: None,
                        text: String::new(),
                        item: None,
                    });
                }
            }
            Event::End(tag) => {
                if let Some(current) = &mut open {
                    let qname = tag.name();
                    let name: &str = qname.as_ref();
                    if depth == current.depth {
                        let current = open.take().ok_or(Stop)?;
                        let values = match current.default {
                            Some(default) => vec![default],
                            None if current.values.is_empty() => vec![current.text],
                            None => current.values,
                        };
                        for value in values {
                            keep(found, current.property, &value);
                        }
                    } else if names.is_rdf(name, "li")
                        && let Some((value, default)) = current.item.take()
                    {
                        if default && current.default.is_none() {
                            current.default = Some(value.clone());
                        }
                        current.values.push(value);
                    }
                }
                depth = depth.checked_sub(1).ok_or(Stop)?;
            }
            Event::Text(text) => push(&mut open, &text.xml10_content()),
            Event::CData(text) => push(&mut open, &text.xml10_content()),
            Event::GeneralRef(reference) => {
                let reference = format!("&{};", AsRef::<str>::as_ref(&reference));
                push(&mut open, &quick_xml::escape::unescape(&reference).map_err(|_| Stop)?);
            }
            // No document type, so no entity can name a file or address.
            Event::DocType(_) => return Err(Stop),
            Event::Eof => return Ok(()),
            _ => {}
        }
    }
}

fn push(open: &mut Option<Open>, text: &str) {
    let Some(current) = open else {
        return;
    };
    let target = match &mut current.item {
        Some((value, _)) => value,
        None => &mut current.text,
    };
    // Past the bound the value is cut, as the parent would cut it.
    let room = MAX_VALUE_BYTES.saturating_sub(target.len());
    target.push_str(&truncated(text.to_owned(), room));
}

fn keep(found: &mut BTreeMap<&'static str, Vec<String>>, property: &'static str, value: &str) {
    let value = value.trim();
    let values = found.entry(property).or_default();
    if !value.is_empty() && values.len() < MAX_LIST {
        values.push(truncated(value.to_owned(), MAX_VALUE_BYTES));
    }
}

/// An invented PDF for tests and fuzz seeds: [`invented_pdf`]'s pages with an
/// Info dictionary of `info` text strings and, when given, `xmp` as its
/// metadata stream. Never a real document.
///
/// [`invented_pdf`]: crate::pdf_text::invented_pdf
#[doc(hidden)]
pub fn invented_pdf_with_details(pages: &[&str], info: &[(&str, &str)], xmp: Option<&str>) -> Vec<u8> {
    let bytes = crate::pdf_text::invented_pdf(pages, None);
    let Ok(mut document) = Document::load_mem(&bytes) else {
        return bytes;
    };
    if !info.is_empty() {
        let mut dictionary = lopdf::Dictionary::new();
        for (key, value) in info {
            dictionary.set(key.as_bytes().to_vec(), lopdf::text_string(value));
        }
        let id = document.add_object(dictionary);
        document.trailer.set("Info", id);
    }
    if let Some(xmp) = xmp {
        let mut dictionary = lopdf::Dictionary::new();
        dictionary.set("Type", Object::Name(b"Metadata".to_vec()));
        dictionary.set("Subtype", Object::Name(b"XML".to_vec()));
        let id = document.add_object(lopdf::Stream::new(dictionary, xmp.as_bytes().to_vec()));
        if let Ok(catalog) = document.catalog_mut() {
            catalog.set("Metadata", id);
        }
    }
    let mut bytes = Vec::new();
    let _ = document.save_to(&mut bytes);
    bytes
}

#[cfg(test)]
#[path = "pdf_details_tests.rs"]
mod tests;
