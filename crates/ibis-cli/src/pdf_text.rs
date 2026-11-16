//! The PDF extractor's child process (decision 0055): `ibis __extract-pdf`.
//!
//! The parent hands over an opened PDF as standard input and nothing else: no
//! path, no environment, no other handle. This side parses the bytes with
//! `lopdf`, a pure-Rust parser without unsafe code, and answers on standard
//! output with each page's text and the printed labels of the document's
//! `/PageLabels`. The parent bounds the time, checks the answer again and
//! decides what may be cited; a crash here only fails this one file.

use ibis_engine::pdf::{Answer, Extracted, LABELS_ONLY, MAX_FILE_BYTES, MAX_PAGES, MAX_TEXT_BYTES, Refusal};
use lopdf::content::Content;
use lopdf::{Dictionary, Document, Encoding, LoadOptions, Object, ObjectId};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{Read, Write};

/// The most one stream may inflate to: page content, object and font streams.
const MAX_STREAM_BYTES: usize = 64 * 1024 * 1024;
/// Address space on Linux, a backstop for a stream bomb.
#[cfg(unix)]
const MAX_MEMORY_BYTES: u64 = 3 * 1024 * 1024 * 1024;
/// Processor seconds, a backstop behind the parent's ten-second deadline.
#[cfg(unix)]
const MAX_CPU_SECONDS: u64 = 20;
/// Nested `/Kids` levels and nodes read from a page label tree.
const MAX_TREE_DEPTH: usize = 32;
const MAX_TREE_NODES: usize = 4096;
const MAX_PREFIX_CHARS: usize = 64;
const MAX_LABEL_BYTES: usize = 1024 * 1024;
// Decimal u64, Roman (up to 16 thousands) and alphabetic suffixes all fit.
const MAX_NUMBER_BYTES: usize = 32;

/// The child process itself: bounded first, then [`child`] over its own
/// standard input and output.
pub fn run_child(arguments: &[OsString]) -> u8 {
    #[cfg(unix)]
    if limit_resources().is_err() {
        return 1;
    }
    child(arguments, std::io::stdin().lock(), std::io::stdout().lock())
}

/// Reads the PDF from `input` and answers on `output`.
pub fn child(arguments: &[OsString], mut input: impl Read, mut output: impl Write) -> u8 {
    let labels_only = match arguments {
        [] => false,
        [flag] if flag == LABELS_ONLY => true,
        _ => return 2,
    };
    let mut bytes = Vec::new();
    let answer = match (&mut input).take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes) {
        Err(_) => Answer::Refused(Refusal::Damaged),
        Ok(_) => match extract(&bytes, labels_only) {
            Ok(extracted) => Answer::Read(extracted),
            Err(refusal) => Answer::Refused(refusal),
        },
    };
    drop(bytes);
    match serde_json::to_writer(&mut output, &answer).and_then(|()| output.flush().map_err(serde_json::Error::io)) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

/// Lower the child's own limits before parsing: no core dumps, processor
/// seconds and, on Linux, address space. The parent's deadline, the per-stream
/// inflation cap and the file-size cap bound the rest everywhere.
#[cfg(unix)]
fn limit_resources() -> nix::Result<()> {
    use nix::sys::resource::{Resource, getrlimit, setrlimit};
    let limits = [
        (Resource::RLIMIT_CPU, MAX_CPU_SECONDS),
        (Resource::RLIMIT_CORE, 0),
        (Resource::RLIMIT_AS, MAX_MEMORY_BYTES),
    ];
    // Darwin may reject a finite address-space limit, so only Linux sets it.
    let count = if cfg!(target_os = "linux") { 3 } else { 2 };
    for (resource, limit) in limits.into_iter().take(count) {
        let (soft, hard) = getrlimit(resource)?;
        let hard = hard.min(limit);
        setrlimit(resource, soft.min(hard), hard)?;
    }
    Ok(())
}

/// A PDF's pages, their text unless `labels_only`, and their labels.
pub fn extract(bytes: &[u8], labels_only: bool) -> Result<Extracted, Refusal> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(Refusal::TooLarge);
    }
    if !ibis_engine::pdf::has_mark(bytes) {
        return Err(Refusal::NotPdf);
    }
    let options = LoadOptions {
        max_decompressed_size: Some(MAX_STREAM_BYTES),
        ..LoadOptions::default()
    };
    let document = Document::load_mem_with_options(bytes, options).map_err(|_| Refusal::Damaged)?;
    let pages: Vec<ObjectId> = document.page_iter().take(MAX_PAGES as usize + 1).collect();
    if pages.is_empty() {
        return Err(Refusal::Damaged);
    }
    if pages.len() > MAX_PAGES as usize {
        return Err(Refusal::TooLarge);
    }
    let count = u32::try_from(pages.len()).map_err(|_| Refusal::TooLarge)?;
    let text = if labels_only {
        None
    } else {
        let mut total = 0usize;
        let mut text = Vec::with_capacity(pages.len());
        for page in &pages {
            let page = page_text(&document, *page);
            total = total.saturating_add(page.len());
            if total > MAX_TEXT_BYTES {
                return Err(Refusal::TooLarge);
            }
            text.push(page);
        }
        Some(text)
    };
    Ok(Extracted {
        pages: count,
        labels: page_labels(&document, pages.len()),
        text,
    })
}

/// The text a page shows, in content order, one line per text line. A page
/// that cannot be decoded gives what was read of it, or nothing.
fn page_text(document: &Document, page: ObjectId) -> String {
    let encodings: BTreeMap<Vec<u8>, Encoding> = document
        .get_page_fonts(page)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(name, font)| {
            Some((
                name,
                font.get_font_encoding_with_limit(document, MAX_STREAM_BYTES).ok()?,
            ))
        })
        .collect();
    let Ok(content) = document
        .get_page_content_with_limit(page, MAX_STREAM_BYTES)
        .and_then(|data| Content::decode(&data))
    else {
        return String::new();
    };
    let mut text = String::new();
    let mut encoding = None;
    let line = |text: &mut String| {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
    };
    for operation in &content.operations {
        let operands = operation.operands.as_slice();
        match operation.operator.as_str() {
            "Tf" => {
                encoding = operands
                    .first()
                    .and_then(|font| font.as_name().ok())
                    .and_then(|font| encodings.get(font));
            }
            "Tj" | "TJ" => show(&mut text, encoding, operands),
            "'" => {
                line(&mut text);
                show(&mut text, encoding, operands);
            }
            "\"" => {
                line(&mut text);
                show(&mut text, encoding, operands.get(2..).unwrap_or_default());
            }
            // A move down starts a line; a move along the same line is a space.
            "Td" | "TD" => match operands.get(1).and_then(number) {
                Some(down) if down != 0.0 => line(&mut text),
                _ if !text.is_empty() && !text.ends_with([' ', '\n']) => text.push(' '),
                _ => (),
            },
            "T*" | "Tm" | "ET" => line(&mut text),
            _ => (),
        }
    }
    clean(text.trim_end())
}

/// Appends the strings `operands` show; a wide gap in a `TJ` array is a space.
fn show(text: &mut String, encoding: Option<&Encoding>, operands: &[Object]) {
    let Some(encoding) = encoding else { return };
    for operand in operands {
        match operand {
            Object::String(bytes, _) => {
                let _ = encoding.write_to_string(bytes, text);
            }
            Object::Array(parts) => show(text, Some(encoding), parts),
            gap if number(gap).is_some_and(|gap| gap < -200.0) && !text.ends_with([' ', '\n']) => text.push(' '),
            _ => (),
        }
    }
}

fn number(object: &Object) -> Option<f64> {
    match object {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some(f64::from(*value)),
        _ => None,
    }
}

/// No control characters but line breaks and tabs: they would only grow the
/// answer and could never be shown.
fn clean(text: &str) -> String {
    text.chars()
        .map(|character| match character {
            '\r' => '\n',
            '\u{c}' => ' ',
            other => other,
        })
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
        .collect()
}

/// One range of a page label tree: its first page, counted from 0, and how
/// its pages are labelled (ISO 32000-1, 12.4.2).
struct LabelRange {
    first: usize,
    style: Option<u8>,
    prefix: String,
    start: u64,
}

/// Each page's label from the catalog's `/PageLabels`, or `None` when there is
/// no tree or it is damaged: a damaged tree gives no labels at all rather
/// than labels that may be wrong. A page before the first range has none.
pub fn page_labels(document: &Document, pages: usize) -> Option<Vec<Option<String>>> {
    if pages > MAX_PAGES as usize {
        return None;
    }
    let tree = document.catalog().ok()?.get(b"PageLabels").ok()?;
    let mut ranges = Vec::new();
    let mut nodes = 0;
    collect_ranges(document, tree, 0, &mut nodes, &mut ranges)?;
    if ranges.is_empty() {
        return None;
    }
    ranges.sort_by_key(|range| range.first);
    if ranges.windows(2).any(|pair| pair[0].first == pair[1].first) {
        return None;
    }
    // Budget every range before allocating or copying any per-page label.
    let mut total = 0usize;
    for (index, range) in ranges.iter().enumerate() {
        let first = range.first.min(pages);
        let end = ranges.get(index + 1).map_or(pages, |next| next.first.min(pages));
        let per_page = range.prefix.len().checked_add(MAX_NUMBER_BYTES)?;
        total = total.checked_add((end - first).checked_mul(per_page)?)?;
        if total > MAX_LABEL_BYTES {
            return None;
        }
    }
    let mut labels = vec![None; pages];
    for (index, range) in ranges.iter().enumerate() {
        let first = range.first.min(pages);
        let end = ranges.get(index + 1).map_or(pages, |next| next.first.min(pages));
        for (offset, slot) in labels[first..end].iter_mut().enumerate() {
            *slot = label(range, offset as u64);
        }
    }
    Some(labels)
}

fn collect_ranges(
    document: &Document,
    node: &Object,
    depth: usize,
    nodes: &mut usize,
    ranges: &mut Vec<LabelRange>,
) -> Option<()> {
    *nodes += 1;
    if depth > MAX_TREE_DEPTH || *nodes > MAX_TREE_NODES {
        return None;
    }
    let node = dictionary(document, node)?;
    if let Ok(kids) = node.get(b"Kids") {
        for kid in document.dereference(kids).ok()?.1.as_array().ok()? {
            collect_ranges(document, kid, depth + 1, nodes, ranges)?;
        }
    }
    if let Ok(numbers) = node.get(b"Nums") {
        let numbers = document.dereference(numbers).ok()?.1.as_array().ok()?;
        if numbers.len() % 2 != 0 || ranges.len() + numbers.len() / 2 > 2 * MAX_PAGES as usize {
            return None;
        }
        for pair in numbers.chunks(2) {
            let first = usize::try_from(document.dereference(&pair[0]).ok()?.1.as_i64().ok()?).ok()?;
            ranges.push(range(document, first, dictionary(document, &pair[1])?)?);
        }
    }
    Some(())
}

fn dictionary<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Dictionary> {
    document.dereference(object).ok()?.1.as_dict().ok()
}

fn range(document: &Document, first: usize, entry: &Dictionary) -> Option<LabelRange> {
    let style = match entry.get(b"S") {
        Err(_) => None,
        Ok(style) => match document.dereference(style).ok()?.1.as_name().ok()? {
            name @ (b"D" | b"R" | b"r" | b"A" | b"a") => Some(name[0]),
            _ => return None,
        },
    };
    let prefix = match entry.get(b"P") {
        Err(_) => String::new(),
        Ok(prefix) => {
            let prefix = document.dereference(prefix).ok()?.1;
            // PDFDocEncoding, UTF-16 or UTF-8 plus BOM: reject an oversized
            // encoded string before the decoder makes another allocation.
            if prefix.as_str().ok()?.len() > 4 * MAX_PREFIX_CHARS + 3 {
                return None;
            }
            let prefix = lopdf::decode_text_string(prefix).ok()?;
            if prefix.chars().count() > MAX_PREFIX_CHARS {
                return None;
            }
            prefix
        }
    };
    let start = match entry.get(b"St") {
        Err(_) => 1,
        Ok(start) => u64::try_from(document.dereference(start).ok()?.1.as_i64().ok()?)
            .ok()
            .filter(|start| *start >= 1)?,
    };
    Some(LabelRange {
        first,
        style,
        prefix,
        start,
    })
}

/// The label of the page `offset` pages into `range`. A number too long to
/// be a citable label gives none.
fn label(range: &LabelRange, offset: u64) -> Option<String> {
    let value = range.start.checked_add(offset)?;
    let number = match range.style {
        None => String::new(),
        Some(b'D') => value.to_string(),
        Some(style @ (b'R' | b'r')) => {
            let roman = roman(value)?;
            if style == b'r' { roman.to_lowercase() } else { roman }
        }
        Some(style) => {
            // A to Z, then AA to ZZ, then AAA: one letter, repeated.
            let repeat = usize::try_from((value - 1) / 26 + 1)
                .ok()
                .filter(|repeat| *repeat <= 16)?;
            let letter = char::from(b'A' + ((value - 1) % 26) as u8);
            let letters = letter.to_string().repeat(repeat);
            if style == b'a' { letters.to_lowercase() } else { letters }
        }
    };
    Some(format!("{}{number}", range.prefix))
}

/// Upper-case Roman numerals; thousands repeat M, up to sixteen of them.
fn roman(mut value: u64) -> Option<String> {
    if value / 1000 > 16 {
        return None;
    }
    const NUMERALS: [(u64, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut roman = String::new();
    for (size, numeral) in NUMERALS {
        while value >= size {
            roman.push_str(numeral);
            value -= size;
        }
    }
    Some(roman)
}

/// One range of an invented PDF's label tree: its first page from 0, its
/// style (`D`, `R`, `r`, `A`, `a` or none), prefix and first number.
#[doc(hidden)]
pub type InventedLabels<'a> = (i64, Option<&'a str>, &'a str, i64);

/// An invented PDF for tests and fuzz seeds: one page per text, in Helvetica,
/// with a `/PageLabels` tree when `labels` gives one. Never a real document.
#[doc(hidden)]
pub fn invented_pdf(pages: &[&str], labels: Option<&[InventedLabels<'_>]>) -> Vec<u8> {
    use lopdf::content::Operation;
    let name = |name: &str| Object::Name(name.as_bytes().to_vec());
    let mut document = Document::with_version("1.7");
    let tree = document.new_object_id();
    let mut font = Dictionary::new();
    font.set("Type", name("Font"));
    font.set("Subtype", name("Type1"));
    font.set("BaseFont", name("Helvetica"));
    font.set("Encoding", name("WinAnsiEncoding"));
    let font = document.add_object(font);
    let mut fonts = Dictionary::new();
    fonts.set("F1", font);
    let mut resources = Dictionary::new();
    resources.set("Font", fonts);
    let resources = document.add_object(resources);
    let mut kids = Vec::new();
    for text in pages {
        let mut operations = vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![name("F1"), 12.into()]),
            Operation::new("Td", vec![72.into(), 720.into()]),
        ];
        for (index, line) in text.lines().enumerate() {
            if index > 0 {
                operations.push(Operation::new("Td", vec![0.into(), (-14).into()]));
            }
            operations.push(Operation::new("Tj", vec![Object::string_literal(line)]));
        }
        operations.push(Operation::new("ET", vec![]));
        let content = Content { operations }.encode().unwrap_or_default();
        let content = document.add_object(lopdf::Stream::new(Dictionary::new(), content));
        let mut page = Dictionary::new();
        page.set("Type", name("Page"));
        page.set("Parent", tree);
        page.set("Contents", content);
        page.set("Resources", resources);
        page.set("MediaBox", vec![0.into(), 0.into(), 612.into(), 792.into()]);
        kids.push(Object::Reference(document.add_object(page)));
    }
    let mut root = Dictionary::new();
    root.set("Type", name("Pages"));
    root.set("Count", i64::try_from(kids.len()).unwrap_or_default());
    root.set("Kids", kids);
    document.objects.insert(tree, Object::Dictionary(root));
    let mut catalog = Dictionary::new();
    catalog.set("Type", name("Catalog"));
    catalog.set("Pages", tree);
    if let Some(labels) = labels {
        let mut numbers = Vec::new();
        for (first, style, prefix, start) in labels {
            let mut entry = Dictionary::new();
            if let Some(style) = style {
                entry.set("S", name(style));
            }
            if !prefix.is_empty() {
                entry.set("P", lopdf::text_string(prefix));
            }
            entry.set("St", *start);
            numbers.push(Object::Integer(*first));
            numbers.push(Object::Dictionary(entry));
        }
        let mut tree = Dictionary::new();
        tree.set("Nums", numbers);
        catalog.set("PageLabels", tree);
    }
    let catalog = document.add_object(catalog);
    document.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    let _ = document.save_to(&mut bytes);
    bytes
}

#[cfg(test)]
#[path = "pdf_text_tests.rs"]
mod tests;
