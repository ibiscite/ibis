//! Ibis's CSV template for references kept in a spreadsheet. The columns are
//! listed in docs/compatibility.md › CSV template; keep the two together.
use std::collections::BTreeMap;

use ibis_core::contract::{LibraryId, Snapshot};
use serde_json::Value;

use super::{Records, date_parts, doi, person};
use crate::{ParsedLibrary, SourceError, WarningCode};

/// The template's columns, each a CSL variable. `type` and `title` are required.
pub(crate) const COLUMNS: [&str; 18] = [
    "type",
    "title",
    "author",
    "editor",
    "issued",
    "container-title",
    "collection-title",
    "volume",
    "issue",
    "page",
    "number",
    "edition",
    "publisher",
    "publisher-place",
    "DOI",
    "ISBN",
    "ISSN",
    "URL",
];

pub(crate) fn load(library: &LibraryId, text: &str, previous: Option<&Snapshot>) -> Result<ParsedLibrary, SourceError> {
    if text.contains('\0') {
        return Err(SourceError::Unreadable);
    }
    let mut records = Records::new(library, previous, "csv");
    if text.trim().is_empty() {
        return Ok(records.parsed);
    }
    let first = text.lines().next().unwrap_or_default();
    // A spreadsheet set to a comma decimal separates cells with semicolons.
    let separator = if first.matches(';').count() > first.matches(',').count() { ';' } else { ',' };
    let mut rows = Rows {
        rest: text,
        line: 1,
        separator,
    };
    let header = match rows.next() {
        Some((_, Ok(header))) => header,
        _ => return Err(SourceError::CsvTemplate),
    };
    let mut columns: Vec<&str> = vec![];
    for name in &header {
        let column = COLUMNS
            .iter()
            .find(|column| column.eq_ignore_ascii_case(name.trim()))
            .ok_or(SourceError::CsvTemplate)?;
        if columns.contains(column) {
            return Err(SourceError::CsvTemplate);
        }
        columns.push(column);
    }
    if !columns.contains(&"type") || !columns.contains(&"title") {
        return Err(SourceError::CsvTemplate);
    }
    for (line, row) in rows {
        if records.count() >= crate::MAX_LIBRARY_RECORDS {
            records.warn(WarningCode::WorkLimit, line);
            break;
        }
        let Ok(cells) = row else {
            records.warn(WarningCode::Syntax, line);
            continue;
        };
        if cells.iter().all(|cell| cell.trim().is_empty()) {
            continue;
        }
        match (cells.len() == columns.len()).then(|| csl(&columns, &cells)).flatten() {
            Some((csl, dropped)) => {
                if dropped {
                    records.warn(WarningCode::Field, line);
                }
                records.push(csl, line, dropped);
            }
            None => records.warn(WarningCode::Syntax, line),
        }
    }
    Ok(records.parsed)
}

/// One row's CSL data, and whether a cell was dropped; `None` skips the row.
fn csl(columns: &[&str], cells: &[String]) -> Option<(BTreeMap<String, Value>, bool)> {
    let mut csl = BTreeMap::new();
    let mut dropped = false;
    for (&column, cell) in columns.iter().zip(cells) {
        let cell = cell.trim();
        if cell.is_empty() {
            continue;
        }
        let value = match column {
            "type" if crate::csl_json::valid_type(cell) => Value::String(cell.into()),
            "type" => return None,
            "author" | "editor" => Value::Array(cell.split(';').filter_map(person).collect()),
            "issued" => {
                let mut parts = cell.splitn(3, '-');
                let (year, month, day) = (parts.next().unwrap_or_default(), parts.next(), parts.next());
                date_parts(year, month, day).unwrap_or_else(|| serde_json::json!({"literal": cell}))
            }
            "DOI" => match doi(cell) {
                Some(doi) => Value::String(doi),
                None => {
                    dropped = true;
                    continue;
                }
            },
            _ => Value::String(cell.into()),
        };
        csl.insert(column.to_owned(), value);
    }
    (csl.contains_key("type") && csl.contains_key("title")).then_some((csl, dropped))
}

/// RFC 4180 rows with the line each starts on. A quoted cell may hold the
/// separator, doubled quotes and line breaks; an unclosed quote ends the file.
struct Rows<'a> {
    rest: &'a str,
    line: usize,
    separator: char,
}

#[derive(Clone, Copy, PartialEq)]
enum Cell {
    Start,
    Plain,
    Quoted,
    Closed,
}

impl Iterator for Rows<'_> {
    type Item = (usize, Result<Vec<String>, ()>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            return None;
        }
        let start = self.line;
        let mut cells = vec![String::new()];
        let mut state = Cell::Start;
        let mut broken = false;
        let mut chars = self.rest.char_indices().peekable();
        let mut end = self.rest.len();
        while let Some((_, c)) = chars.next() {
            let newline = c == '\r' || c == '\n';
            // `\r\n` is one line break.
            if c == '\r' && chars.peek().is_some_and(|(_, next)| *next == '\n') && state != Cell::Quoted {
                chars.next();
            }
            match state {
                Cell::Quoted if c == '"' && chars.peek().is_some_and(|(_, next)| *next == '"') => {
                    chars.next();
                    cells.last_mut()?.push('"');
                }
                Cell::Quoted if c == '"' => state = Cell::Closed,
                Cell::Quoted => {
                    if c == '\n' || (c == '\r' && chars.peek().is_none_or(|(_, next)| *next != '\n')) {
                        self.line += 1;
                    }
                    cells.last_mut()?.push(c);
                }
                _ if newline => {
                    end = chars.peek().map_or(self.rest.len(), |(next, _)| *next);
                    self.line += 1;
                    break;
                }
                _ if c == self.separator => {
                    cells.push(String::new());
                    state = Cell::Start;
                }
                Cell::Start if c == '"' => state = Cell::Quoted,
                // Text after a closing quote does not fit the template.
                Cell::Closed => broken = true,
                Cell::Start | Cell::Plain => {
                    cells.last_mut()?.push(c);
                    state = Cell::Plain;
                }
            }
        }
        self.rest = &self.rest[end..];
        Some((start, if state == Cell::Quoted || broken { Err(()) } else { Ok(cells) }))
    }
}
