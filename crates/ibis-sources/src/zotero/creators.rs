// Particle/suffix conversion is derived from Zotero utilities_item.js,
// Copyright © 2021 Corporation for Digital Scholarship and
// Copyright (c) 2009–2019 Frank Bennett, AGPL-3.0-or-later option.
// Upstream files and complete notices are in vendor/zotero.
use super::{ItemType, MappingWarning, ZoteroItem, warn};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) fn map(
    item: &ZoteroItem,
    kind: &ItemType,
    names: &BTreeMap<String, String>,
    csl: &mut BTreeMap<String, Value>,
    warnings: &mut Vec<MappingWarning>,
) {
    let primary = kind
        .creator_types
        .iter()
        .find(|kind| kind.primary)
        .map(|kind| kind.creator_type.as_str());
    for creator in &item.creators {
        let role = names
            .get(&creator.creator_type)
            .map(String::as_str)
            .or_else(|| (primary == Some(creator.creator_type.as_str())).then_some("author"));
        let Some(role) = role else {
            warn(warnings, MappingWarning::SkippedCreator);
            continue;
        };
        let name = if !creator.name.is_empty() {
            json!({"literal": creator.name})
        } else if creator.field_mode == 1 && !creator.last_name.is_empty() && creator.first_name.is_empty() {
            json!({"literal": creator.last_name})
        } else if !creator.last_name.is_empty() || !creator.first_name.is_empty() {
            let mut name = BTreeMap::from([
                ("family".into(), json!(creator.last_name)),
                ("given".into(), json!(creator.first_name)),
            ]);
            if !creator.last_name.is_empty() && !creator.first_name.is_empty() {
                if creator.last_name.starts_with('"') && creator.last_name.ends_with('"') && creator.last_name.len() > 1
                {
                    name.insert(
                        "family".into(),
                        json!(&creator.last_name[1..creator.last_name.len() - 1]),
                    );
                } else {
                    particles(&mut name, &creator.last_name, &creator.first_name);
                }
            }
            json!(name)
        } else {
            warn(warnings, MappingWarning::SkippedCreator);
            continue;
        };
        if let Some(names) = csl.entry(role.into()).or_insert_with(|| json!([])).as_array_mut() {
            names.push(name);
        }
    }
}

fn lower_particle(value: &str) -> bool {
    // JS's dot-based first-character replacement is a no-op across a line
    // terminator; upstream then compares the casing of the entire candidate.
    if value.contains(['\r', '\n', '\u{2028}', '\u{2029}']) {
        return value.to_uppercase() != value;
    }
    value
        .chars()
        .find(|c| !matches!(c, '-' | '\'' | 'ʻ' | '’') && !c.is_whitespace())
        .is_some_and(|c| c.to_uppercase().collect::<String>() != c.to_string())
}

fn family_prefix(value: &str) -> Option<usize> {
    if value.starts_with(' ') {
        return None;
    }
    if let Some(space) = value.find(' ') {
        let end = space + value[space..].bytes().take_while(|b| *b == b' ').count();
        return (end < value.len()).then_some(end);
    }
    value
        .char_indices()
        .rev()
        .find(|(index, c)| *index + c.len_utf8() < value.len() && matches!(c, '-' | '\'' | 'ʻ' | '’'))
        .map(|(index, c)| index + c.len_utf8())
}

fn particles(name: &mut BTreeMap<String, Value>, family: &str, given: &str) {
    let mut remaining = family;
    let mut end = 0;
    let last_family_break = family.rfind(['\r', '\n', '\u{2028}', '\u{2029}']);
    while let Some(length) = family_prefix(remaining) {
        if !lower_particle(&remaining[..length]) || last_family_break.is_some_and(|index| index >= end + length) {
            break;
        }
        end += length;
        remaining = &remaining[length..];
    }
    name.insert("family".into(), json!(remaining));
    let raw_particle = &family[..end];
    let mut particle = raw_particle.trim().to_owned();
    if raw_particle.ends_with(' ') && particle.ends_with(['\'', '’']) {
        particle.push(' ');
    }
    if !particle.is_empty() {
        name.insert("non-dropping-particle".into(), json!(particle));
    }
    let mut given = given;
    let mut et_al = false;
    if let Some(comma) = given.find(',') {
        let start = given[..comma].trim_end_matches(char::is_whitespace).len();
        let mut after = comma + 1;
        while given.as_bytes().get(after) == Some(&b'!') {
            after += 1;
        }
        while given[after..].chars().next().is_some_and(char::is_whitespace) {
            after += given[after..].chars().next().map_or(0, char::len_utf8);
        }
        let suffix = &given[after..];
        if suffix.replace('.', "") == "et al" {
            name.insert("dropping-particle".into(), json!(suffix));
            name.insert("comma-dropping-particle".into(), json!(","));
            et_al = true;
        } else {
            name.insert("suffix".into(), json!(suffix));
            if given[start..after].chars().filter(|c| !c.is_whitespace()).count() == 2 {
                name.insert("comma-suffix".into(), json!(true));
            }
        }
        given = &given[..start];
    }
    let mut remaining_given = given;
    let mut particle_start = given.len();
    let first_given_break = given.find(['\r', '\n', '\u{2028}', '\u{2029}']);
    while let Some(space) = remaining_given.rfind(' ') {
        let part = &remaining_given[space + 1..];
        if part.is_empty() || !lower_particle(part) || first_given_break.is_some_and(|index| index < space) {
            break;
        }
        remaining_given = remaining_given[..space].trim_end_matches(' ');
        particle_start = remaining_given.len();
    }
    name.insert("given".into(), json!(remaining_given));
    if particle_start < given.len() {
        name.insert("dropping-particle".into(), json!(given[particle_start..].trim()));
    } else if !et_al {
        name.remove("dropping-particle");
    }
}
