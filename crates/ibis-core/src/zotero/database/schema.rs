//! Validate schema expansion before cloning its type and field aliases.

use std::collections::BTreeSet;

use serde_json::Value;

use super::{Item, Schema};

const MAX_SCHEMA_BYTES: usize = 2 * 1024 * 1024;
pub(super) const MAX_IDENTIFIER_BYTES: usize = 128;
const MAX_BINDINGS: usize = 4096;
const MAX_EXPANDED_BYTES: usize = 4 * 1024 * 1024;

#[derive(Default)]
struct Expansion {
    bindings: usize,
    bytes: usize,
}

impl Expansion {
    fn reserve(&mut self, identifiers: &[&str]) -> Option<()> {
        if identifiers.iter().any(|name| {
            name.is_empty()
                || name.len() > MAX_IDENTIFIER_BYTES
                || !name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
        }) {
            return None;
        }
        self.bindings = self.bindings.checked_add(1)?;
        // Two copies cover reverse aliases/date sets; 512 bytes cover the
        // collection nodes, String/Vec headers and temporary row storage.
        let bytes = identifiers
            .iter()
            .try_fold(512usize, |total, name| total.checked_add(name.len().checked_mul(2)?))?;
        self.bytes = self.bytes.checked_add(bytes)?;
        (self.bindings <= MAX_BINDINGS && self.bytes <= MAX_EXPANDED_BYTES).then_some(())
    }
}

impl Schema {
    /// Read Zotero's inflated global schema and its type-specific mappings.
    /// Refuse missing mappings or unsupported identifier/fanout sizes before
    /// materializing any aliases. Known Zotero/CSL identifiers are short ASCII.
    pub fn new(
        global_schema: &[u8],
        base_fields: impl IntoIterator<Item = (String, String, String)>,
        primary_creators: impl IntoIterator<Item = (String, String)>,
    ) -> Option<Schema> {
        if global_schema.len() > MAX_SCHEMA_BYTES {
            return None;
        }
        let schema: Value = serde_json::from_slice(global_schema).ok()?;
        let csl = schema.get("csl")?;
        let types = csl.get("types")?.as_object()?;
        let text = csl.pointer("/fields/text")?.as_object()?;
        let dates = csl.pointer("/fields/date")?.as_object()?;
        let names = csl.get("names")?.as_object()?;
        let mut expansion = Expansion::default();
        for aliases in [types, text] {
            for (variable, fields) in aliases {
                expansion.reserve(&[variable])?;
                for field in fields.as_array()? {
                    expansion.reserve(&[variable, field.as_str()?])?;
                }
            }
        }
        for aliases in [dates, names] {
            for (variable, field) in aliases {
                expansion.reserve(&[variable, field.as_str()?])?;
            }
        }
        let date_fields = schema
            .pointer("/meta/fields")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .filter(|(_, meta)| meta.get("type").and_then(Value::as_str) == Some("date"))
            .map(|(field, _)| field);
        for field in date_fields.clone() {
            expansion.reserve(&[field])?;
        }
        // Move caller-owned rows only after validating each reservation;
        // never trust an input iterator's allocation size hint.
        let mut base = Vec::new();
        for row in base_fields {
            expansion.reserve(&[&row.0, &row.1, &row.2])?;
            base.push(row);
        }
        let mut primary = Vec::new();
        for row in primary_creators {
            expansion.reserve(&[&row.0, &row.1])?;
            primary.push(row);
        }

        let strings = |value: &Value| -> Vec<String> {
            value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        };
        let mut result = Schema::default();
        for (csl_type, zotero_types) in types {
            for zotero_type in strings(zotero_types) {
                result.types.insert(zotero_type, csl_type.clone());
            }
        }
        for (variable, fields) in text {
            let fields = strings(fields);
            for field in &fields {
                result.reverse.insert(field.clone(), variable.clone());
            }
            result.text.push((variable.clone(), fields));
        }
        for (variable, field) in dates {
            let field = field.as_str()?.to_string();
            result.reverse.insert(field.clone(), variable.clone());
            result.dates.push((variable.clone(), field));
        }
        for (creator, variable) in names {
            result.names.insert(creator.clone(), variable.as_str()?.to_string());
        }
        let date_bases: BTreeSet<String> = date_fields.cloned().collect();
        for (item_type, base, field) in base {
            if date_bases.contains(&base) {
                result.date_fields.insert(field.clone());
            }
            result.base.insert((item_type, base), field);
        }
        result.date_fields.extend(date_bases);
        result.primary.extend(primary);
        Some(result)
    }

    pub(super) fn field<'a>(&self, item: &'a Item, field: &str) -> Option<&'a str> {
        item.fields
            .get(field)
            .or_else(|| {
                if item.item_type.len() > MAX_IDENTIFIER_BYTES {
                    return None;
                }
                self.base
                    .get(&(item.item_type.clone(), field.to_string()))
                    .and_then(|own| item.fields.get(own))
            })
            .map(String::as_str)
            .filter(|value| !value.is_empty())
    }
}
