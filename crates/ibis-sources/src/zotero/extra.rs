// Derived from Zotero utilities_item.js extraToCSL (AGPL-3.0-or-later),
// Copyright © 2021 Corporation for Digital Scholarship. See vendor/zotero.
use std::collections::BTreeMap;

use super::{MappingWarning, Schema, dates, public_url, warn};
use serde_json::Value;

const STANDARD: &str = "abstract accessed annote archive archive-place author authority call-number chapter-number citation-label citation-number collection-editor collection-number collection-title composer container container-author container-title container-title-short dimensions director edition editor editorial-director event event-date event-place first-reference-note-number genre illustrator interviewer issue issued jurisdiction keyword language locator medium note number number-of-pages number-of-volumes original-author original-date original-publisher original-publisher-place original-title page page-first publisher publisher-place recipient references reviewed-author reviewed-title scale section source status submitted title title-short translator type version volume year-suffix";

pub(super) fn normalize(extra: &str, schema: &Schema) -> String {
    let lines: Vec<_> = extra.split_inclusive(['\n', '\r', '\u{2028}', '\u{2029}']).collect();
    let mut consumed_through = None;
    lines
        .iter()
        .enumerate()
        .map(|(index, &line)| {
            if consumed_through.is_some_and(|end| index <= end) {
                return line.to_owned();
            }
            let Some((label, value)) = line.split_once(':') else {
                return line.to_owned();
            };
            if !label.chars().all(|c| c.is_ascii_alphabetic() || c == ' ' || c == '-')
                || label.is_empty()
                || value.is_empty() && index + 1 == lines.len()
            {
                return line.to_owned();
            }
            // The upstream /m expression allows whitespace after the colon to span
            // lines. Its greedy match consumes the next nonblank line unchanged.
            if value.trim().is_empty() {
                consumed_through = lines
                    .iter()
                    .enumerate()
                    .skip(index + 1)
                    .find(|(_, next)| !next.trim().is_empty())
                    .map(|(end, _)| end);
                if consumed_through.is_none()
                    && !value
                        .chars()
                        .any(|c| !matches!(c, '\r' | '\n' | '\u{2028}' | '\u{2029}'))
                {
                    return line.to_owned();
                }
            }
            let lower = label.to_ascii_lowercase().replace(' ', "-");
            let field = if STANDARD.split_ascii_whitespace().any(|field| field == lower) {
                lower
            } else if matches!(lower.as_str(), "doi" | "isbn" | "issn" | "pmcid" | "pmid" | "url") {
                lower.to_ascii_uppercase()
            } else if lower == "archive-location" {
                "archive_location".into()
            } else {
                let mut zotero_field = label.to_owned();
                if let Some(index) = label
                    .as_bytes()
                    .windows(2)
                    .position(|pair| pair[0] == b' ' && pair[1].is_ascii_uppercase())
                {
                    zotero_field.remove(index);
                }
                if zotero_field.as_bytes().get(1).is_some_and(|b| b.is_ascii_lowercase()) {
                    zotero_field[..1].make_ascii_lowercase();
                }
                // As in schema.js, the final mapping wins; date mappings follow text.
                schema
                    .csl
                    .fields
                    .date
                    .iter()
                    .find(|(_, field)| **field == zotero_field)
                    .map(|(variable, _)| variable.clone())
                    .or_else(|| {
                        schema
                            .csl
                            .fields
                            .text
                            .iter()
                            .rev()
                            .find(|(_, fields)| fields.contains(&zotero_field))
                            .map(|(variable, _)| variable.clone())
                    })
                    .unwrap_or_else(|| label.into())
            };
            format!("{field}:{value}")
        })
        .collect::<Vec<_>>()
        .join("")
}

fn field_line(line: &str) -> Option<(&str, &str)> {
    let (label, value) = line.split_once(':')?;
    // citeproc's note-field syntax; unparsed prose stops overrides after line 1.
    if label.is_empty()
        || !(label
            .bytes()
            .all(|b| b.is_ascii_lowercase() || matches!(b, b'-' | b'_'))
            || label.bytes().all(|b| b.is_ascii_uppercase()))
        || value.contains('}')
        || value.trim_start().is_empty()
    {
        return None;
    }
    Some((label, value.trim()))
}

pub(super) fn date_overrides(note: &str) -> (String, BTreeMap<String, Value>) {
    let mut lines: Vec<_> = note.split('\n').map(str::to_owned).collect();
    let mut overrides = BTreeMap::new();
    for (index, line) in lines.iter_mut().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let Some((label, value)) = field_line(line) else {
            if index == 0 {
                continue;
            }
            break;
        };
        if matches!(
            label,
            "issued" | "event-date" | "original-date" | "publication-date" | "available-date" | "submitted"
        ) && let Some(date) = dates::extra_date(value)
        {
            overrides.insert(label.into(), date);
            line.clear();
        }
    }
    (lines.join("\n"), overrides)
}

pub(super) fn without_private_urls(note: &str, warnings: &mut Vec<MappingWarning>) -> String {
    note.split('\n')
        .map(|line| {
            // Removing the value, but retaining a blank line, keeps Zotero's first
            // line/prose boundary behavior for remaining note-field overrides.
            if field_line(line).is_some_and(|(label, value)| label == "URL" && !public_url(value)) {
                warn(warnings, MappingWarning::PrivateUrl);
                ""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
