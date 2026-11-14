//! The first EndNote XML slice: articles, books and chapters, bibliographic data only.
use std::collections::BTreeMap;

use ibis_core::contract::{LibraryId, Snapshot};
use quick_xml::{Reader, events::Event};
use serde_json::{Value, json};

use super::{RecordProblem, Records, date_parts, doi, person};
use crate::{ParsedLibrary, SourceError, WarningCause, WarningCode};

type Fields = BTreeMap<String, Vec<String>>;

pub(crate) fn looks_like_endnote(text: &str) -> bool {
    let mut reader = Reader::from_str(text);
    let mut depth = 0;
    let mut root = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(tag)) => {
                if depth == 0 {
                    root = tag.name().as_ref() == "xml";
                    if !root {
                        return false;
                    }
                } else if depth == 1 && root && tag.name().as_ref() == "records" {
                    return true;
                }
                depth += 1;
            }
            Ok(Event::Empty(tag)) if depth == 1 && root && tag.name().as_ref() == "records" => return true,
            Ok(Event::End(_)) => {
                if depth <= 1 {
                    return false;
                }
                depth -= 1;
            }
            Ok(Event::Eof) | Err(_) => return false,
            _ => {}
        }
    }
}

pub(crate) fn load(library: &LibraryId, text: &str, previous: Option<&Snapshot>) -> Result<ParsedLibrary, SourceError> {
    if !text.chars().all(xml_char) {
        return Err(SourceError::InvalidData);
    }
    let mut reader = Reader::from_str(text);
    reader.config_mut().expand_empty_elements = true;
    reader.config_mut().check_comments = true;
    let mut records = Records::new(library, previous, "endnote");
    let mut path = Vec::new();
    let mut fields = Fields::new();
    let mut value = String::new();
    let (mut root, mut recognized) = (false, false);
    let (mut line, mut record_line) = (1, 1);
    loop {
        let offset = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(|_| SourceError::InvalidData)?;
        let event_line = line;
        line += text[offset..reader.buffer_position() as usize]
            .bytes()
            .filter(|b| *b == b'\n')
            .count();
        match event {
            Event::Start(tag) => {
                if path.is_empty() {
                    if root {
                        return Err(SourceError::InvalidData);
                    }
                    if tag.name().as_ref() != "xml" {
                        return Err(SourceError::UnknownFormat);
                    }
                    root = true;
                }
                // Validate even unused attributes; no entity resolver or I/O exists here.
                for attribute in tag.attributes() {
                    let attribute = attribute.map_err(|_| SourceError::InvalidData)?;
                    if attribute.value.contains('<') {
                        return Err(SourceError::InvalidData);
                    }
                    unescape(&attribute.value)?;
                }
                let name = tag.name().as_ref().to_owned();
                if name != "style" {
                    value.clear();
                }
                path.push(name);
                if path.len() > 64 {
                    return Err(SourceError::InvalidData);
                }
                if path == ["xml", "records"] {
                    if recognized {
                        return Err(SourceError::InvalidData);
                    }
                    recognized = true;
                }
                if path == ["xml", "records", "record"] {
                    record_line = event_line;
                    if !records.start(record_line) {
                        return Ok(records.finish());
                    }
                    fields.clear();
                }
            }
            Event::End(tag) => {
                if tag.name().as_ref() != "style" {
                    if path == ["xml", "records", "record"] {
                        match csl(&fields) {
                            Ok((csl, uncertain)) => {
                                if uncertain {
                                    records.warn(WarningCode::Field, record_line);
                                }
                                records.push(csl, record_line, uncertain);
                            }
                            Err(problem) => {
                                records.skip(record_line, problem);
                                records.label_warnings(
                                    fields.get("titles/title").and_then(|v| v.first()).map(String::as_str),
                                );
                            }
                        }
                    } else if path.len() > 3 && path[..3] == ["xml", "records", "record"] && !value.trim().is_empty() {
                        fields.entry(path[3..].join("/")).or_default().push(value.trim().into());
                    }
                    value.clear();
                }
                path.pop();
            }
            Event::Text(text) => {
                if path.is_empty() && !text.trim().is_empty() {
                    return Err(SourceError::InvalidData);
                }
                value.push_str(&text.xml10_content());
            }
            Event::CData(_) | Event::GeneralRef(_) if path.is_empty() => return Err(SourceError::InvalidData),
            Event::CData(text) => value.push_str(&text.xml10_content()),
            Event::GeneralRef(reference) => {
                let escaped = format!("&{};", reference.as_ref());
                value.push_str(&unescape(&escaped)?);
            }
            Event::Decl(declaration)
                if offset != 0 || declaration.version().map_err(|_| SourceError::InvalidData)?.as_ref() != "1.0" =>
            {
                return Err(SourceError::InvalidData);
            }
            // Reject all DTDs, including external subsets and parameter entities.
            Event::DocType(_) => return Err(SourceError::InvalidData),
            Event::Eof => {
                if !path.is_empty() {
                    return Err(SourceError::InvalidData);
                }
                return if recognized {
                    Ok(records.finish())
                } else {
                    Err(SourceError::UnknownFormat)
                };
            }
            _ => {}
        }
    }
}

fn xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}

fn unescape(text: &str) -> Result<std::borrow::Cow<'_, str>, SourceError> {
    let decoded = quick_xml::escape::unescape(text).map_err(|_| SourceError::InvalidData)?;
    if !decoded.chars().all(xml_char) {
        return Err(SourceError::InvalidData);
    }
    Ok(decoded)
}

fn csl(fields: &Fields) -> Result<(BTreeMap<String, Value>, bool), RecordProblem> {
    let one = |names: &[&str]| -> Result<Option<&str>, RecordProblem> {
        let mut values = names.iter().flat_map(|name| fields.get(*name).into_iter().flatten());
        let first = values.next();
        if values.any(|other| Some(other) != first) {
            return Err((WarningCause::Conflicting, None));
        }
        Ok(first.map(String::as_str))
    };
    let kind = match one(&["ref-type"])? {
        Some("17") => "article-journal",
        Some("6") => "book",
        Some("5") => "chapter",
        _ => return Err((WarningCause::UnknownType, Some("ref-type"))),
    };
    let title = one(&["titles/title"])?.ok_or((WarningCause::MissingField, Some("title")))?;
    let mut csl = BTreeMap::from([("type".into(), json!(kind)), ("title".into(), json!(title))]);
    let contributors = match kind {
        "book" => &[
            ("contributors/authors/author", "author"),
            ("contributors/secondary-authors/author", "collection-editor"),
            ("contributors/tertiary-authors/author", "editor"),
        ][..],
        _ => &[
            ("contributors/authors/author", "author"),
            ("contributors/secondary-authors/author", "editor"),
        ][..],
    };
    for (source, target) in contributors {
        let people: Vec<_> = fields
            .get(*source)
            .into_iter()
            .flatten()
            .filter_map(|name| endnote_person(name))
            .collect();
        if !people.is_empty() {
            csl.insert((*target).into(), json!(people));
        }
    }
    for (source, target) in [
        ("pages", if kind == "book" { "number-of-pages" } else { "page" }),
        ("volume", "volume"),
        ("edition", "edition"),
        ("number", if kind == "article-journal" { "issue" } else { "number" }),
        ("publisher", "publisher"),
        ("pub-location", "publisher-place"),
    ] {
        if let Some(value) = one(&[source])? {
            csl.insert(target.into(), json!(value));
        }
    }
    if let Some(title) = one(&["titles/secondary-title", "periodical/full-title"])? {
        csl.insert(
            if kind == "book" {
                "collection-title"
            } else {
                "container-title"
            }
            .into(),
            json!(title),
        );
    }
    let mut uncertain = false;
    if let Some(written) = one(&["electronic-resource-num"])? {
        if let Some(doi) = doi(written) {
            csl.insert("DOI".into(), json!(doi));
        } else {
            uncertain = true;
        }
    }
    let year = one(&["dates/year"])?;
    let publication = one(&["dates/pub-dates/date"])?;
    let mut issued = year.and_then(|year| date_parts(year, None, None));
    if let Some(date) = publication {
        let parts: Vec<_> = date.split(['/', '-']).collect();
        if date_parts(parts[0], None, None).is_some() && year.is_some_and(|year| parts[0] != year) {
            return Err((WarningCause::Conflicting, Some("year")));
        }
        let full = (parts.len() <= 3)
            .then(|| date_parts(parts[0], parts.get(1).copied(), parts.get(2).copied()))
            .flatten();
        uncertain |= full.is_none();
        issued = full.or(issued);
    }
    uncertain |= year.is_some() && issued.is_none();
    if let Some(issued) = issued {
        csl.insert("issued".into(), issued);
    }
    Ok((csl, uncertain))
}

fn endnote_person(name: &str) -> Option<Value> {
    let name = name.trim();
    if let Some((first, rest)) = name.split_once(",,") {
        Some(json!({"literal": format!("{first},{rest}")}))
    } else if let Some(corporate) = name.strip_suffix(',') {
        let corporate = corporate.trim();
        (!corporate.is_empty()).then(|| json!({"literal": corporate}))
    } else {
        person(name)
    }
}
