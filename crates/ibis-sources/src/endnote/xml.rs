//! EndNote XML, as EndNote's File › Export and its scripting `export`
//! command write it: `<xml><records><record>…`. Text only: DTDs are refused,
//! so no entity can name a file or address, and every name, attribute list,
//! depth and the total work are bounded before the reader stores them.
use std::collections::BTreeMap;

use quick_xml::{Reader, events::Event};

/// A record's values by element path below `<record>` (`titles/title`), in
/// order, with the two attributes that say which library it came from.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct XmlRecord {
    pub fields: BTreeMap<String, Vec<String>>,
    /// `<database path="…">`: the library file EndNote exported from.
    pub database: Option<String>,
    /// `<foreign-keys><key db-id="…">`: EndNote's id for that library.
    pub db_id: Option<String>,
}

impl XmlRecord {
    pub(crate) fn one(&self, name: &str) -> Option<&str> {
        self.fields
            .get(name)
            .and_then(|values| values.first())
            .map(String::as_str)
    }
}

/// Why the text is not an EndNote library: the whole read is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum XmlError {
    /// Not well-formed, or not `<xml><records>`.
    Invalid,
    /// Past a size, depth or work bound.
    TooLarge,
}

const NAME_BYTES: usize = 256;
const ATTRIBUTES: usize = 128;
const DEPTH: usize = 64;
const WORK: usize = 256 * 1024 * 1024;
const RECORDS: usize = 200_000;

// quick-xml stores a start name before returning its event; check the next
// markup's name length before handing it over.
fn name_fits(text: &str, offset: usize) -> bool {
    let Some(markup) = text.as_bytes().get(offset..).and_then(|rest| rest.strip_prefix(b"<")) else {
        return true;
    };
    let markup = markup.strip_prefix(b"/").unwrap_or(markup);
    if matches!(markup.first(), Some(b'!' | b'?')) {
        return true;
    }
    markup
        .iter()
        .take_while(|b| !b.is_ascii_whitespace() && !matches!(b, b'/' | b'>'))
        .take(NAME_BYTES + 1)
        .count()
        <= NAME_BYTES
}

fn xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}

fn unescape(text: &str) -> Result<std::borrow::Cow<'_, str>, XmlError> {
    let decoded = quick_xml::escape::unescape(text).map_err(|_| XmlError::Invalid)?;
    if !decoded.chars().all(xml_char) {
        return Err(XmlError::Invalid);
    }
    Ok(decoded)
}

/// Every record in `text`, or why none can be trusted.
pub(crate) fn records(text: &str) -> Result<Vec<XmlRecord>, XmlError> {
    let text = text.trim_start_matches('\u{feff}');
    if !text.chars().all(xml_char) {
        return Err(XmlError::Invalid);
    }
    let mut reader = Reader::from_str(text);
    reader.config_mut().expand_empty_elements = true;
    reader.config_mut().check_comments = true;
    let mut records = Vec::new();
    let mut path: Vec<String> = Vec::new();
    let mut record = XmlRecord::default();
    let mut value = String::new();
    let (mut root, mut recognized) = (false, false);
    let mut work = 0usize;
    loop {
        let offset = usize::try_from(reader.buffer_position()).map_err(|_| XmlError::TooLarge)?;
        if !name_fits(text, offset) {
            return Err(XmlError::TooLarge);
        }
        let event = reader.read_event().map_err(|_| XmlError::Invalid)?;
        let end = usize::try_from(reader.buffer_position()).map_err(|_| XmlError::TooLarge)?;
        work = work.saturating_add(end.saturating_sub(offset)).saturating_add(128);
        if work > WORK {
            return Err(XmlError::TooLarge);
        }
        match event {
            Event::Start(tag) => {
                if path.is_empty() {
                    if root || tag.name().as_ref() != "xml" {
                        return Err(XmlError::Invalid);
                    }
                    root = true;
                }
                let name = tag.name().as_ref().to_owned();
                let within = path.len() >= 3 && path[..3] == ["xml", "records", "record"];
                // Validate even attributes that are not used.
                for (index, attribute) in tag.attributes().enumerate() {
                    let attribute = attribute.map_err(|_| XmlError::Invalid)?;
                    if index >= ATTRIBUTES || attribute.key.as_ref().len() > NAME_BYTES {
                        return Err(XmlError::TooLarge);
                    }
                    let raw: &str = &attribute.value;
                    if raw.contains('<') {
                        return Err(XmlError::Invalid);
                    }
                    let decoded = unescape(raw)?;
                    if within && path.len() == 3 && name == "database" && attribute.key.as_ref() == "path" {
                        record.database = Some(decoded.into_owned());
                    } else if within
                        && path.len() == 4
                        && path[3] == "foreign-keys"
                        && name == "key"
                        && attribute.key.as_ref() == "db-id"
                    {
                        record.db_id = Some(decoded.into_owned());
                    }
                }
                if name != "style" {
                    value.clear();
                }
                path.push(name);
                if path.len() > DEPTH {
                    return Err(XmlError::TooLarge);
                }
                if path == ["xml", "records"] {
                    if recognized {
                        return Err(XmlError::Invalid);
                    }
                    recognized = true;
                }
                if path == ["xml", "records", "record"] {
                    if records.len() >= RECORDS {
                        return Err(XmlError::TooLarge);
                    }
                    record = XmlRecord::default();
                }
            }
            Event::End(tag) => {
                // Styled runs (`<style face="bold">`) join their element's text.
                if tag.name().as_ref() != "style" {
                    if path == ["xml", "records", "record"] {
                        records.push(std::mem::take(&mut record));
                    } else if path.len() > 3 && path[..3] == ["xml", "records", "record"] && !value.trim().is_empty() {
                        let name = path[3..].join("/");
                        work = work.saturating_add(name.len() + value.len());
                        if work > WORK {
                            return Err(XmlError::TooLarge);
                        }
                        record.fields.entry(name).or_default().push(value.trim().into());
                    }
                    value.clear();
                }
                path.pop();
            }
            Event::Text(text) => {
                if path.is_empty() && !text.trim().is_empty() {
                    return Err(XmlError::Invalid);
                }
                value.push_str(&text.xml10_content());
            }
            Event::CData(_) | Event::GeneralRef(_) if path.is_empty() => return Err(XmlError::Invalid),
            Event::CData(text) => value.push_str(&text.xml10_content()),
            Event::GeneralRef(reference) => {
                value.push_str(&unescape(&format!("&{};", reference.as_ref()))?);
            }
            Event::Decl(declaration)
                if offset != 0 || declaration.version().map_err(|_| XmlError::Invalid)?.as_ref() != "1.0" =>
            {
                return Err(XmlError::Invalid);
            }
            // Every DTD, including external subsets and parameter entities.
            Event::DocType(_) => return Err(XmlError::Invalid),
            Event::Eof => {
                return if path.is_empty() && recognized {
                    Ok(records)
                } else {
                    Err(XmlError::Invalid)
                };
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = include_str!("../../../../fixtures/endnote/invented.xml");

    #[test]
    fn styled_text_joins_and_library_attributes_are_kept() {
        let xml = r#"<xml><records><record><database name="A.enl" path="/invented/A.enl">A.enl</database><foreign-keys><key app="EN" db-id="invented-d">7</key></foreign-keys><rec-number>7</rec-number><titles><title><style face="normal">Invented </style><style face="italic">tides</style> &amp; more</title></titles></record></records></xml>"#;
        let records = records(xml).unwrap_or_else(|_| panic!("reads"));
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].one("titles/title"), Some("Invented tides & more"));
        assert_eq!(records[0].one("rec-number"), Some("7"));
        assert_eq!(records[0].database.as_deref(), Some("/invented/A.enl"));
        assert_eq!(records[0].db_id.as_deref(), Some("invented-d"));
        assert_eq!(super::records(XML).map(|records| records.len()), Ok(5));
    }

    #[test]
    fn damaged_text_dtds_and_bounds_refuse_the_whole_read() {
        for bad in [
            XML.replace("</records>", ""),
            XML.replace("</title>", "</wrong>"),
            format!("{XML}<xml/>"),
            XML.replace("Invented Series", "undefined &entity;"),
            XML.replace("Invented Series", "invalid \0"),
            XML.replace("Invented Series", "invalid &#1;"),
            format!("{XML}stray"),
            "<html/>".into(),
            "<records><record/></records>".into(),
        ] {
            assert_eq!(records(&bad), Err(XmlError::Invalid), "{bad:.60}");
        }
        for declaration in [
            "<!DOCTYPE xml SYSTEM 'file:///invented-entity-must-not-be-opened'>",
            "<!DOCTYPE xml [<!ENTITY external SYSTEM 'file:///invented-entity-must-not-be-opened'>]>",
            "<!DOCTYPE xml [<!ENTITY % external SYSTEM 'https://example.org/must-not-be-requested'>%external;]>",
        ] {
            let xml = format!(
                "{declaration}<xml><records><record><titles><title>&external;</title></titles></record></records></xml>"
            );
            assert_eq!(records(&xml), Err(XmlError::Invalid));
        }
        let long = "a".repeat(257);
        for xml in [
            format!("<xml><records><{long}></{long}></records></xml>"),
            format!("<xml><records><record {long}=\"x\"/></records></xml>"),
            format!(
                "<xml><records><record {}/></records></xml>",
                (0..129).map(|i| format!("a{i}=\"x\" ")).collect::<String>()
            ),
            format!("<xml><records>{}</records></xml>", "<a>".repeat(70)),
        ] {
            assert_eq!(records(&xml), Err(XmlError::TooLarge));
        }
        assert_eq!(records("<xml><records/></xml>"), Ok(vec![]));
    }
}
