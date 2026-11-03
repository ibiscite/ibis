//! What a chosen `.bib` holds, read for checking rather than for searching.
//!
//! The loader drops repeated and damaged entries, so its records cannot prove
//! that a key is absent or unique. This scan keeps every occurrence with its
//! line, and every reason its answer cannot be trusted. It also finds one
//! entry's original text with what that text needs to stand on its own.

use std::collections::{BTreeSet, HashMap};

use super::Budget;
use super::lexer::{Block, Part, RawEntry, parse_entry, parse_string, split_blocks};

/// Where a key was found. An `@xdata` entry only supplies fields, so its key
/// cannot be cited even though it is written in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Occurrence {
    pub line: usize,
    pub helper: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct KeyScan {
    occurrences: HashMap<String, Vec<Occurrence>>,
    /// Lower-cased keys to their spellings; BibTeX reads keys without case.
    spellings: HashMap<String, BTreeSet<String>>,
    /// Keys named by a BibLaTeX `ids` field, which Biber also resolves.
    aliases: HashMap<String, usize>,
    /// Lines the scan could not read, in file order.
    pub damaged: Vec<usize>,
    /// The scan stopped before the end of the file.
    pub limited: bool,
}

impl KeyScan {
    pub fn occurrences(&self, key: &str) -> &[Occurrence] {
        self.occurrences.get(key).map_or(&[], Vec::as_slice)
    }
    /// Another spelling of `key` that differs only by case.
    pub fn other_spelling(&self, key: &str) -> bool {
        self.spellings
            .get(&key.to_lowercase())
            .is_some_and(|spellings| spellings.iter().any(|spelling| spelling != key))
    }
    /// The first line of a spelling of `key` that differs only by case.
    pub fn spelling_line(&self, key: &str) -> Option<usize> {
        let spellings = self.spellings.get(&key.to_lowercase())?;
        spellings
            .iter()
            .filter(|spelling| *spelling != key)
            .find_map(|spelling| self.occurrences(spelling).first())
            .map(|occurrence| occurrence.line)
    }
    pub fn alias_line(&self, key: &str) -> Option<usize> {
        self.aliases.get(key).copied()
    }
    pub fn complete(&self) -> bool {
        !self.limited && self.damaged.is_empty()
    }
}

/// The literal text of a value, or `None` when it uses a macro.
fn literal(parts: &[Part<'_>]) -> Option<String> {
    let mut text = String::new();
    for part in parts {
        match part {
            Part::Text(value) => text.push_str(value),
            Part::Number(value) => text.push_str(value),
            Part::Macro(_) => return None,
        }
    }
    Some(text)
}

fn keys_in(value: &str) -> impl Iterator<Item = &str> {
    value.split(',').map(str::trim).filter(|key| !key.is_empty())
}

pub(crate) fn scan(text: &str) -> KeyScan {
    let mut scan = KeyScan::default();
    let budget = Budget::for_input(text.len());
    let (blocks, limited) = split_blocks(text);
    scan.limited = limited;
    for block in &blocks {
        if !budget.charge(block.text.len().saturating_add(256)) {
            scan.limited = true;
            break;
        }
        if !block.complete {
            scan.damaged.push(block.line);
            continue;
        }
        match block.kind.as_str() {
            "comment" | "preamble" => {}
            "string" if parse_string(block).is_none() => scan.damaged.push(block.line),
            "string" => {}
            _ => match parse_entry(block) {
                Ok(entry) => record(&mut scan, &entry),
                Err(_) => scan.damaged.push(block.line),
            },
        }
    }
    scan
}

fn record(scan: &mut KeyScan, entry: &RawEntry<'_>) {
    scan.occurrences.entry(entry.key.clone()).or_default().push(Occurrence {
        line: entry.line,
        helper: entry.kind == "xdata",
    });
    scan.spellings
        .entry(entry.key.to_lowercase())
        .or_default()
        .insert(entry.key.clone());
    for field in entry.fields.iter().filter(|field| field.name == "ids") {
        match literal(&field.parts) {
            Some(value) => {
                for alias in keys_in(&value) {
                    scan.aliases.entry(alias.to_owned()).or_insert(entry.line);
                }
            }
            // An alias list that depends on a macro could name any key.
            None => scan.damaged.push(entry.line),
        }
    }
}

/// Why an entry's original text cannot stand on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryProblem {
    NotFound,
    Repeated,
    /// A `crossref`, `xdata` or macro it needs is missing, damaged or redefined.
    Dependency,
}

/// Months BibTeX styles define themselves, so no `@string` is needed.
fn built_in(name: &str) -> bool {
    matches!(
        name,
        "jan" | "feb" | "mar" | "apr" | "may" | "jun" | "jul" | "aug" | "sep" | "oct" | "nov" | "dec"
    )
}

struct Definition<'b, 's> {
    index: usize,
    block: &'b Block<'s>,
    parts: Vec<Part<'s>>,
}

/// One entry exactly as written, after the `@string`s it uses and before the
/// `crossref` and `xdata` entries it inherits from, as BibTeX needs them.
/// Every line is original text: nothing is rewritten, so the key stays the same.
pub(crate) fn entry_text(text: &str, key: &str) -> Result<String, EntryProblem> {
    let (blocks, _) = split_blocks(text);
    let mut entries: HashMap<String, Vec<(usize, RawEntry<'_>)>> = HashMap::new();
    let mut definitions: HashMap<String, Vec<Definition<'_, '_>>> = HashMap::new();
    for (index, block) in blocks.iter().enumerate().filter(|(_, block)| block.complete) {
        match block.kind.as_str() {
            "comment" | "preamble" => {}
            "string" => {
                if let Some((name, parts)) = parse_string(block) {
                    definitions
                        .entry(name)
                        .or_default()
                        .push(Definition { index, block, parts });
                }
            }
            _ => {
                if let Ok(entry) = parse_entry(block) {
                    entries.entry(entry.key.clone()).or_default().push((index, entry));
                }
            }
        }
    }
    match entries.get(key).map(Vec::as_slice) {
        None | Some([]) => return Err(EntryProblem::NotFound),
        Some([(_, entry)]) if entry.kind != "xdata" => {}
        Some([_]) => return Err(EntryProblem::NotFound),
        Some(_) => return Err(EntryProblem::Repeated),
    }

    let mut needed = vec![key.to_owned()];
    let mut parents = BTreeSet::new();
    let mut strings = BTreeSet::new();
    while let Some(next) = needed.pop() {
        let [(index, entry)] = entries.get(&next).map(Vec::as_slice).ok_or(EntryProblem::Dependency)? else {
            return Err(EntryProblem::Dependency);
        };
        if next != key {
            parents.insert(*index);
        }
        let mut macros: Vec<(String, usize)> = vec![];
        for field in &entry.fields {
            for part in &field.parts {
                if let Part::Macro(name) = part {
                    macros.push((name.clone(), *index));
                }
            }
            if matches!(field.name.as_str(), "crossref" | "xdata") {
                let value = literal(&field.parts).ok_or(EntryProblem::Dependency)?;
                for parent in keys_in(&value) {
                    // Each parent is followed once, so a cycle ends here.
                    let seen = entries
                        .get(parent)
                        .is_some_and(|found| found.iter().any(|(index, _)| parents.contains(index)));
                    if parent != key && !seen {
                        needed.push(parent.to_owned());
                    }
                }
            }
        }
        while let Some((name, used_at)) = macros.pop() {
            let name = name.to_ascii_lowercase();
            let Some(found) = definitions.get(&name) else {
                if built_in(&name) {
                    continue;
                }
                return Err(EntryProblem::Dependency);
            };
            // A redefined macro means different things at different lines.
            let [definition] = found.as_slice() else {
                return Err(EntryProblem::Dependency);
            };
            if definition.index >= used_at {
                return Err(EntryProblem::Dependency);
            }
            if strings.insert(definition.index) {
                for part in &definition.parts {
                    if let Part::Macro(inner) = part {
                        macros.push((inner.clone(), definition.index));
                    }
                }
            }
        }
    }

    let mut lines: Vec<&str> = vec![];
    let string_text: HashMap<usize, &str> = definitions
        .values()
        .flatten()
        .map(|definition| (definition.index, definition.block.text))
        .collect();
    lines.extend(strings.iter().filter_map(|index| string_text.get(index).copied()));
    let entry_text = |index: usize| blocks.get(index).map(|block| block.text);
    let own = entries
        .get(key)
        .and_then(|found| found.first())
        .map(|(index, _)| *index);
    lines.extend(own.and_then(entry_text));
    lines.extend(parents.iter().filter_map(|index| entry_text(*index)));
    Ok(lines.join("\n\n") + "\n")
}
