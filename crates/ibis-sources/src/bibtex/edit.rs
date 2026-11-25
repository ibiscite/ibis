//! Changes to a `.bib` file's text that keep every byte outside what they
//! change (decision 0055). Only a file the key scan reads completely is
//! edited: a damaged entry could be anything, including the key a new entry
//! would take.

use std::ops::Range;

use super::lexer::{Part, field_spans, split_blocks};
use super::project::{KeyScan, scan};

/// Why a file or an entry cannot be edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditProblem {
    /// Some entry could not be read, or the file is past a read limit.
    Damaged,
    NotFound,
    /// Two entries have this key, perhaps in different letter case.
    Repeated,
    /// The value uses a macro or joins parts, so it is not plain text to edit.
    NotLiteral,
}

/// A `.bib` file read for editing.
pub struct BibFile<'a> {
    text: &'a str,
    scan: KeyScan,
    entries: Vec<Entry<'a>>,
}

/// One entry and where each of its parts is written in the file.
pub struct Entry<'a> {
    text: &'a str,
    /// Lowercased entry type, such as `article`.
    pub kind: String,
    pub key: String,
    block: Range<usize>,
    key_end: usize,
    fields: Vec<Field<'a>>,
}

struct Field<'a> {
    name: String,
    whole: Range<usize>,
    value: Range<usize>,
    parts: Vec<Part<'a>>,
}

impl<'a> BibFile<'a> {
    pub fn read(text: &'a str) -> Result<Self, EditProblem> {
        let scan = scan(text);
        if !scan.complete() {
            return Err(EditProblem::Damaged);
        }
        let (blocks, limited) = split_blocks(text);
        if limited {
            return Err(EditProblem::Damaged);
        }
        let mut entries = Vec::new();
        for block in blocks
            .iter()
            .filter(|block| !matches!(block.kind.as_str(), "comment" | "preamble" | "string"))
        {
            let start = block.text.as_ptr() as usize - text.as_ptr() as usize;
            let body = block.body.as_ptr() as usize - text.as_ptr() as usize;
            let (key, fields) = field_spans(block.body).ok_or(EditProblem::Damaged)?;
            let shift = |range: Range<usize>| range.start + body..range.end + body;
            entries.push(Entry {
                text,
                kind: block.kind.clone(),
                key: block.body[key.clone()].to_owned(),
                block: start..start + block.text.len(),
                key_end: key.end + body,
                fields: fields
                    .into_iter()
                    .map(|field| Field {
                        name: field.name,
                        whole: shift(field.whole),
                        value: shift(field.value),
                        parts: field.parts,
                    })
                    .collect(),
            });
        }
        Ok(Self { text, scan, entries })
    }

    /// Whether a new entry may not take `key`: an entry, `ids` alias or
    /// commented entry already uses it in some letter case.
    pub fn key_taken(&self, key: &str) -> bool {
        self.scan.taken(key)
    }

    /// The one entry `key` names.
    pub fn entry(&self, key: &str) -> Result<&Entry<'a>, EditProblem> {
        let mut found = self.entries.iter().filter(|entry| entry.key == key);
        match (found.next(), found.next()) {
            (None, _) => Err(EditProblem::NotFound),
            (Some(_), Some(_)) => Err(EditProblem::Repeated),
            (Some(_), None) if self.scan.other_spelling(key) => Err(EditProblem::Repeated),
            (Some(entry), None) => Ok(entry),
        }
    }

    /// The file with `entry` added after everything else, on lines of its own.
    pub fn append(&self, entry: &str) -> String {
        let newline = newline(self.text);
        let mut out = String::with_capacity(self.text.len() + entry.len() + 4);
        out.push_str(self.text);
        if !self.text.is_empty() && !self.text.ends_with('\n') {
            out.push_str(newline);
        }
        if !self.text.trim_start_matches('\u{feff}').is_empty() {
            out.push_str(newline);
        }
        out.push_str(&entry.replace('\n', newline));
        out.push_str(newline);
        out
    }
}

impl<'a> Entry<'a> {
    /// The entry exactly as written.
    pub fn text(&self) -> &'a str {
        &self.text[self.block.clone()]
    }

    pub fn has(&self, name: &str) -> bool {
        self.fields.iter().any(|field| field.name == name)
    }

    /// The text inside a field's delimiters, when its value is one literal.
    pub fn literal(&self, name: &str) -> Option<Result<&'a str, EditProblem>> {
        let field = self.fields.iter().find(|field| field.name == name)?;
        Some(match field.parts.as_slice() {
            [Part::Text(_)] => {
                let written = &self.text[field.value.clone()];
                Ok(&written[1..written.len() - 1])
            }
            [Part::Number(_)] => Ok(&self.text[field.value.clone()]),
            _ => Err(EditProblem::NotLiteral),
        })
    }

    /// The whole file with this entry's fields changed: `Some` sets a field
    /// to a value written with its delimiters, `None` removes it. Every byte
    /// outside the fields named stays as it was.
    pub fn edited(&self, changes: &[(&str, Option<String>)]) -> String {
        let newline = newline(self.text());
        let removed = |field: &Field<'_>| {
            changes
                .iter()
                .any(|(name, value)| *name == field.name && value.is_none())
        };
        let mut edits: Vec<(Range<usize>, String)> = Vec::new();
        for field in &self.fields {
            if let Some((_, value)) = changes.iter().find(|(name, _)| *name == field.name) {
                match value {
                    Some(value) => edits.push((field.value.clone(), value.clone())),
                    None => edits.push((self.removal(field), String::new())),
                }
            }
        }
        let added: Vec<_> = changes
            .iter()
            .filter_map(|(name, value)| Some((*name, value.as_ref()?)))
            .filter(|(name, _)| !self.has(name))
            .collect();
        if !added.is_empty() {
            let at = self
                .fields
                .iter()
                .rfind(|field| !removed(field))
                .map_or(self.key_end, |field| field.whole.end);
            let indent = self.indent();
            let text = added
                .iter()
                .map(|(name, value)| format!(",{newline}{indent}{name} = {value}"))
                .collect::<String>();
            edits.push((at..at, text));
        }
        edits.sort_by_key(|(range, _)| (range.start, range.end));
        let mut out = String::with_capacity(self.text.len() + 256);
        let mut from = 0;
        for (range, text) in edits {
            // Removing neighbouring fields on one line, or a field written
            // twice, gives overlapping ranges: the removals join (#53).
            let start = range.start.max(from);
            out.push_str(&self.text[from..start]);
            out.push_str(&text);
            from = from.max(range.end);
        }
        out.push_str(&self.text[from..]);
        out
    }

    /// A field with its line when it has one to itself, and the comma after it.
    fn removal(&self, field: &Field<'_>) -> Range<usize> {
        let text = self.text;
        let line_start = text[..field.whole.start].rfind('\n').map_or(0, |at| at + 1);
        let own_line = text[line_start..field.whole.start].trim().is_empty();
        let mut end = field.whole.end;
        let rest = &text[end..self.block.end];
        let spaces = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        let comma_after = rest[spaces..].starts_with(',');
        if comma_after {
            end += spaces + 1;
        }
        if !own_line && !comma_after {
            // The last field on a shared line takes the comma before it.
            let before = text[..field.whole.start].trim_end_matches([' ', '\t']);
            let start = before.strip_suffix(',').unwrap_or(before).len();
            return start.max(self.key_end)..end;
        }
        if own_line {
            let rest = &text[end..self.block.end];
            let blank = rest.len() - rest.trim_start_matches([' ', '\t', '\r']).len();
            if rest[blank..].starts_with('\n') {
                end += blank + 1;
            }
            line_start.max(self.key_end)..end
        } else {
            field.whole.start..end
        }
    }

    /// The space before the first field on its line, or two spaces.
    fn indent(&self) -> &'a str {
        self.fields
            .iter()
            .find_map(|field| {
                let line_start = self.text[..field.whole.start].rfind('\n')? + 1;
                let before = &self.text[line_start..field.whole.start];
                (line_start > self.block.start && before.trim().is_empty()).then_some(before)
            })
            .unwrap_or("  ")
    }
}

fn newline(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

/// The fields of one entry as `name` and the value as written, in order, as
/// a writer made them. `None` when `text` is not one readable entry.
pub fn written_fields(text: &str) -> Option<Vec<(String, String)>> {
    let file = BibFile::read(text).ok()?;
    let [entry] = file.entries.as_slice() else {
        return None;
    };
    Some(
        entry
            .fields
            .iter()
            .map(|field| (field.name.clone(), text[field.value.clone()].to_owned()))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "% Invented library\n@string{pub = {Synthetic Press}}\n\n@article{morgan2025,\n    title = {Synthetic noise},\n    journal = pub,\n    year = 2025,\n}\n\n@book{taylor2026, title={Synthetic signals}, year={2026}}\n";

    fn read(text: &str) -> BibFile<'_> {
        BibFile::read(text).unwrap_or_else(|problem| panic!("{problem:?}"))
    }

    #[test]
    fn a_changed_field_keeps_every_other_byte() {
        let file = read(FILE);
        let entry = file.entry("morgan2025").unwrap_or_else(|problem| panic!("{problem:?}"));
        let edited = entry.edited(&[("title", Some("{Synthetic quiet}".into()))]);
        assert_eq!(edited, FILE.replace("{Synthetic noise}", "{Synthetic quiet}"));
        assert_eq!(entry.literal("journal"), Some(Err(EditProblem::NotLiteral)));
        assert_eq!(entry.literal("year"), Some(Ok("2025")));
    }

    #[test]
    fn neighbouring_and_repeated_fields_are_removed_together() {
        for (text, changes, kept) in [
            ("@misc{x, a=1, b=2}", vec![("a", None), ("b", None)], vec![]),
            ("@misc{x, note=f, note=t}", vec![("note", None)], vec![]),
            ("@misc{x, a=1, b=2, c=3}", vec![("a", None), ("b", None)], vec!["c"]),
        ] {
            let file = read(text);
            let edited = file.entry("x").unwrap().edited(&changes);
            let again = read(&edited);
            let entry = again.entry("x").unwrap();
            for (name, _) in &changes {
                assert!(!entry.has(name), "{text} kept {name}: {edited}");
            }
            for name in kept {
                assert!(entry.has(name), "{text} lost {name}: {edited}");
            }
        }
    }

    #[test]
    fn keys_are_taken_in_any_case_and_damaged_files_are_not_edited() {
        let file = read(FILE);
        assert!(file.key_taken("Morgan2025"));
        assert!(!file.key_taken("morgan2025a"));
        assert_eq!(file.entry("absent").err(), Some(EditProblem::NotFound));
        let repeated = format!("{FILE}@misc{{Morgan2025, title={{Other}}}}\n");
        assert_eq!(read(&repeated).entry("morgan2025").err(), Some(EditProblem::Repeated));
        assert_eq!(
            BibFile::read("@article{broken, title={open\n").err(),
            Some(EditProblem::Damaged)
        );
    }

    #[test]
    fn appended_entries_keep_the_file_and_its_line_endings() {
        let crlf = FILE.replace('\n', "\r\n");
        let file = read(&crlf);
        let added = file.append("@misc{new,\n  title = {New},\n}");
        assert_eq!(added, format!("{crlf}\r\n@misc{{new,\r\n  title = {{New}},\r\n}}\r\n"));
        assert!(added.starts_with(&crlf));
        assert_eq!(read("").append("@misc{new}"), "@misc{new}\n");
        assert_eq!(read("% end").append("@misc{new}"), "% end\n\n@misc{new}\n");
    }
}
