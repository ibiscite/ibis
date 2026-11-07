//! RIS, as reference managers and research portals export it (#1531). Ibis
//! owns this small reader: published crates merge secondary authors into
//! authors or accept a record without `ER`.
use std::collections::BTreeMap;

use ibis_core::contract::{LibraryId, Snapshot};
use serde_json::Value;

use super::{Records, Serial, date_parts, doi, person, serials};
use crate::{ParsedLibrary, SourceError, WarningCode};

/// Whether the text starts as RIS does, with a `TY` line.
pub(crate) fn looks_like_ris(text: &str) -> bool {
    lines(text.trim_start_matches('\u{feff}'))
        .find(|(_, line)| !line.trim().is_empty())
        .is_some_and(|(_, line)| tag(line).is_some_and(|(tag, _)| tag == "TY"))
}

pub(crate) fn load(library: &LibraryId, text: &str, previous: Option<&Snapshot>) -> Result<ParsedLibrary, SourceError> {
    if text.contains('\0') {
        // UTF-16 read as bytes: each character carries a zero byte.
        return Err(SourceError::Unreadable);
    }
    let mut records = Records::new(library, previous, "ris");
    let mut open: Option<(usize, Vec<(&str, String)>)> = None;
    let mut stray = false;
    let mut found = false;
    for (number, line) in lines(text) {
        let Some((tag, value)) = tag(line) else {
            match &mut open {
                // A long value some exporters wrap onto the next line.
                Some((_, fields)) if !line.trim().is_empty() => match fields.last_mut() {
                    Some((last, text)) if *last != "TY" => {
                        text.push(' ');
                        text.push_str(line.trim());
                    }
                    _ => fields.push(("", String::new())),
                },
                _ => {}
            }
            continue;
        };
        match (tag, &mut open) {
            ("TY", _) => {
                found = true;
                stray = false;
                if let Some((start, _)) = open.take() {
                    records.warn(WarningCode::Syntax, start);
                }
                if !records.start(number) {
                    return Ok(records.parsed);
                }
                open = Some((number, vec![(tag, value.to_owned())]));
            }
            ("ER", Some(_)) => {
                let Some((start, fields)) = open.take() else { continue };
                match csl(&fields) {
                    Some((csl, dropped)) => {
                        if dropped {
                            records.warn(WarningCode::Field, start);
                        }
                        records.push(csl, start, dropped);
                    }
                    None => records.warn(WarningCode::Syntax, start),
                }
            }
            (_, Some((_, fields))) => fields.push((tag, value.to_owned())),
            // Fields with no `TY` before them: a record that lost its start.
            (_, None) => {
                if !stray {
                    records.warn(WarningCode::Syntax, number);
                }
                stray = true;
            }
        }
    }
    if let Some((start, _)) = open {
        records.warn(WarningCode::Syntax, start);
    }
    if !found && !text.trim().is_empty() {
        return Err(SourceError::InvalidData);
    }
    Ok(records.parsed)
}

/// Lines split at `\r\n`, `\n` or `\r`, numbered from 1.
fn lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut rest = Some(text);
    std::iter::from_fn(move || {
        let current = rest?;
        match current.find(['\r', '\n']) {
            Some(end) => {
                let skip = if current[end..].starts_with("\r\n") { 2 } else { 1 };
                rest = Some(&current[end + skip..]);
                Some(&current[..end])
            }
            None => {
                rest = None;
                (!current.is_empty()).then_some(current)
            }
        }
    })
    .enumerate()
    .map(|(index, line)| (index + 1, line))
}

/// `XY  - value`: two capitals or a capital and a digit, two spaces, a hyphen.
fn tag(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    let valid = bytes.len() >= 5
        && bytes[0].is_ascii_uppercase()
        && (bytes[1].is_ascii_uppercase() || bytes[1].is_ascii_digit())
        && &bytes[2..5] == b"  -"
        && bytes.get(5).is_none_or(|b| *b == b' ');
    valid.then(|| (&line[..2], line[5..].trim()))
}

#[derive(Clone, Copy, PartialEq)]
enum Group {
    /// Articles: T2 is the periodical.
    Article,
    /// Chapters and papers: T2 is the book or proceedings.
    Part,
    /// Books: T2 is the series, SP the number of pages.
    Book,
    /// Web pages and posts: T2 is the site.
    Web,
    Other,
}

/// The allowlist: an unknown type is refused, never defaulted. `CONF` waits
/// for a tested exporter, because portals use it for papers and whole volumes.
fn kind(ty: &str) -> Option<(&'static str, Group)> {
    use Group::*;
    Some(match ty {
        "JOUR" | "EJOUR" | "INPR" => ("article-journal", Article),
        "MGZN" => ("article-magazine", Article),
        "NEWS" => ("article-newspaper", Article),
        "BOOK" | "EBOOK" | "EDBOOK" => ("book", Book),
        "CHAP" | "ECHAP" => ("chapter", Part),
        "CPAPER" => ("paper-conference", Part),
        "ENCYC" => ("entry-encyclopedia", Part),
        "DICT" => ("entry-dictionary", Part),
        "THES" => ("thesis", Other),
        "RPRT" => ("report", Other),
        "WEB" | "ELEC" => ("webpage", Web),
        "BLOG" => ("post-weblog", Web),
        "DATA" => ("dataset", Other),
        "COMP" => ("software", Other),
        "PAT" => ("patent", Other),
        "MAP" => ("map", Other),
        "UNPB" | "MANSCPT" => ("manuscript", Other),
        "STAND" => ("standard", Other),
        "VIDEO" => ("motion_picture", Other),
        "SOUND" => ("song", Other),
        "ART" => ("graphic", Other),
        "PCOMM" => ("personal_communication", Other),
        "GEN" => ("document", Other),
        _ => return None,
    })
}

/// A record's CSL data, and whether a field was dropped; `None` refuses it.
fn csl(fields: &[(&str, String)]) -> Option<(BTreeMap<String, Value>, bool)> {
    if fields.iter().any(|(tag, _)| tag.is_empty()) {
        return None;
    }
    let all = |tags: &[&str]| -> Vec<&str> {
        fields
            .iter()
            .filter(|(tag, value)| tags.contains(tag) && !value.is_empty())
            .map(|(_, value)| value.as_str())
            .collect()
    };
    // One value, the same however often it is given; two different ones refuse.
    let one = |tags: &[&str]| -> Result<Option<&str>, ()> {
        let values = all(tags);
        match values.first() {
            Some(first) if values.iter().any(|value| !value.eq_ignore_ascii_case(first)) => Err(()),
            first => Ok(first.copied()),
        }
    };
    let (kind, group) = kind(one(&["TY"]).ok()??)?;
    let mut csl = BTreeMap::new();
    let mut dropped = false;
    csl.insert("type".into(), Value::String(kind.into()));
    let book_title = if group == Group::Book {
        &["TI", "T1", "CT", "BT"][..]
    } else {
        &["TI", "T1", "CT"]
    };
    text(&mut csl, "title", one(book_title).ok()?);

    let names = |tags: &[&str]| -> Option<Value> {
        // A person given twice, however spaced, is one person, in the order first given.
        let mut seen = std::collections::HashSet::new();
        let people: Vec<Value> = all(tags)
            .into_iter()
            .filter_map(person)
            .filter(|person| seen.insert(person.to_string()))
            .collect();
        (!people.is_empty()).then_some(Value::Array(people))
    };
    let put_names = |csl: &mut BTreeMap<String, Value>, field: &str, tags: &[&str]| {
        if let Some(people) = names(tags) {
            csl.insert(field.into(), people);
        }
    };
    // Secondary names are never folded into authors; their role is the type's.
    put_names(&mut csl, "author", &["AU", "A1"]);
    let edited = matches!(group, Group::Book | Group::Part);
    put_names(&mut csl, "editor", if edited { &["ED", "A2"] } else { &["ED"] });
    if edited {
        put_names(&mut csl, "collection-editor", &["A3"]);
        put_names(&mut csl, "translator", &["A4"]);
    }

    match date(&all(&["PY", "Y1"]), &all(&["DA"]))? {
        (Some(issued), lost) => {
            csl.insert("issued".into(), issued);
            dropped |= lost;
        }
        (None, lost) => dropped |= lost,
    }

    let containers = one(&["T2", "JF"]).ok()?;
    match group {
        Group::Article => {
            text(&mut csl, "container-title", containers);
            // Never expanded: an abbreviation stays the short title.
            text(
                &mut csl,
                "container-title-short",
                all(&["J2", "JO", "JA"]).first().copied(),
            );
        }
        Group::Part => {
            text(&mut csl, "container-title", one(&["T2", "BT"]).ok()?);
            text(&mut csl, "collection-title", one(&["T3"]).ok()?);
        }
        Group::Book => text(&mut csl, "collection-title", one(&["T2", "T3"]).ok()?),
        Group::Web => text(&mut csl, "container-title", containers),
        Group::Other => {}
    }

    let start = one(&["SP"]).ok()?;
    let end = one(&["EP"]).ok()?;
    match (start, end) {
        (Some(pages), None) if group == Group::Book => text(&mut csl, "number-of-pages", Some(pages)),
        (Some(start), None) => text(&mut csl, "page", Some(start)),
        (Some(start), Some(end)) => {
            let page = match start.split_once(['-', '–', '—']) {
                // A whole range given twice must agree with its end.
                Some((first, written)) if !same_end(first.trim(), written.trim(), end) => return None,
                Some(_) => start.to_owned(),
                None => format!("{start}-{end}"),
            };
            text(&mut csl, "page", Some(&page));
        }
        (None, end) => dropped |= end.is_some(),
    }
    text(&mut csl, "number", one(&["C7"]).ok()?);
    for (field, tag) in [
        ("volume", "VL"),
        ("issue", "IS"),
        ("publisher", "PB"),
        ("publisher-place", "CY"),
        ("edition", "ET"),
        ("title-short", "ST"),
        ("language", "LA"),
    ] {
        match one(&[tag]) {
            Ok(value) => text(&mut csl, field, value),
            Err(()) => dropped = true,
        }
    }
    let serial = match group {
        Group::Article => Some(Serial::Issn),
        Group::Book | Group::Part => Some(Serial::Isbn),
        Group::Web | Group::Other => None,
    };
    if let Some(kind) = serial {
        let (value, lost) = serials(kind, &all(&["SN"]));
        text(&mut csl, kind.field(), value.as_deref());
        dropped |= lost;
    }
    let dois = all(&["DO"]);
    if let Some(first) = dois.first() {
        match doi(first) {
            Some(value) if dois.iter().all(|other| doi(other).as_ref() == Some(&value)) => {
                text(&mut csl, "DOI", Some(&value));
            }
            _ => dropped = true,
        }
    }
    // Exports list file links beside web addresses; only a public one is kept.
    let public = all(&["UR"]).into_iter().find(|url| ibis_core::citable::public_url(url));
    text(&mut csl, "URL", public);
    Some((csl, dropped))
}

/// Whether `end` is the end of `first`–`written`, which may be abbreviated
/// (`1654–6` ends at 1656).
fn same_end(first: &str, written: &str, end: &str) -> bool {
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
    written == end
        || digits(first)
            && digits(written)
            && written.len() < first.len()
            && end == format!("{}{written}", &first[..first.len() - written.len()])
}

fn text(csl: &mut BTreeMap<String, Value>, field: &str, value: Option<&str>) {
    if let Some(value) = value {
        csl.insert(field.to_owned(), Value::String(value.to_owned()));
    }
}

/// `issued` from `PY`/`Y1` and `DA`, and whether a value was dropped. Every
/// value must give the same year, and fuller dates must extend shorter ones;
/// otherwise the record is refused (`None`). Exact parts win over text kept as
/// written unless they give only the year.
fn date(years: &[&str], dates: &[&str]) -> Option<(Option<Value>, bool)> {
    let mut dropped = false;
    let mut year = None;
    let mut parts: Vec<Vec<Value>> = vec![];
    // The parts each value states, a text date's known month and day included.
    let mut stated: Vec<Vec<Value>> = vec![];
    let mut literal: Option<&str> = None;
    for &text in years.iter().chain(dates) {
        let mut pieces = text.split('/');
        let written_year = pieces.next().unwrap_or_default();
        if written_year.len() != 4 || !written_year.bytes().all(|b| b.is_ascii_digit()) {
            // `DA  - MAY` and the like name no year: nothing to cite by.
            dropped = true;
            continue;
        }
        if year
            .replace(written_year)
            .is_some_and(|earlier| earlier != written_year)
        {
            return None;
        }
        let month = pieces.next().filter(|month| !month.is_empty());
        let day = pieces.next().filter(|day| !day.is_empty());
        let other = pieces.collect::<Vec<_>>().join("/");
        let known = [(month, day), (month, None), (None, None)]
            .into_iter()
            .find_map(|(month, day)| date_parts(written_year, month, day))?;
        stated.push(known["date-parts"][0].as_array()?.clone());
        match date_parts(written_year, month, day) {
            Some(value) if other.trim().is_empty() => parts.push(value["date-parts"][0].as_array()?.clone()),
            // Text Ibis cannot split is kept as written, never guessed.
            _ if literal.is_some_and(|earlier| earlier != text) => return None,
            _ => literal = Some(text),
        }
    }
    stated.sort_by_key(Vec::len);
    if stated
        .iter()
        .any(|shorter| !stated.last().is_some_and(|fullest| fullest.starts_with(shorter)))
    {
        return None;
    }
    parts.sort_by_key(Vec::len);
    let fullest = parts.last();
    let issued = match (fullest, literal) {
        (Some(fullest), Some(text)) if fullest.len() == 1 => serde_json::json!({"literal": text}),
        (Some(fullest), _) => serde_json::json!({"date-parts": [fullest]}),
        (None, Some(text)) => serde_json::json!({"literal": text}),
        (None, None) => return Some((None, dropped)),
    };
    Some((Some(issued), dropped))
}
