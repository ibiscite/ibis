//! Zotero items as references.
//!
//! Zotero describes each item twice: its own record (`data`: item key, type,
//! citation key, Extra, tags, trash state) and a CSL-JSON rendering
//! (`csljson`). Both access paths, the local API while Zotero runs (#13) and
//! a copy of its database while it is closed (#14), produce that pair, so
//! this one mapping serves both and they cannot drift apart.
//!
//! Notes, attachments, annotations and items in the trash are not citable
//! references and are left out without a warning, as Zotero's own citation
//! dialog does. Anything else that cannot be read becomes a [`LoadWarning`].

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde_json::{Map, Value};

pub mod api;
pub mod libraries;

use crate::model::{
    Issued, LoadResult, LoadWarning, Name, PartialDate, RecordId, Reference, WarningKind, days_in_month,
};

/// Which Zotero library items belong to. Identity is the library plus the
/// item key, never a citation key (ADR 0006).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Library {
    /// The personal library ("My Library").
    User,
    /// A group library, by its numeric group ID.
    Group(u64),
}

impl fmt::Display for Library {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Library::User => f.write_str("zotero:user"),
            Library::Group(id) => write!(f, "zotero:group:{id}"),
        }
    }
}

/// Zotero item types that are not citable references.
const NOT_CITABLE: [&str; 3] = ["note", "attachment", "annotation"];

/// CSL name variables kept in dedicated fields.
const NAME_FIELDS: [&str; 2] = ["author", "editor"];

/// CSL string variables kept in dedicated fields.
const TEXT_FIELDS: [&str; 6] = ["title", "container-title", "DOI", "URL", "abstract", "language"];

/// Variables that describe the export, not the work.
const IGNORED: [&str; 4] = ["id", "type", "citation-key", "keyword"];

/// Turn Zotero items (each an object with `key`, `data` and `csljson`, as
/// the API returns them with `include=data,csljson`) into references.
///
/// An item key seen twice keeps the first item. A citation key shared by
/// several items keeps them all, since their identity differs, but is
/// reported because a citation would be ambiguous.
pub fn load_items(library: Library, items: &[Value]) -> LoadResult {
    let mut result = LoadResult::default();
    let mut seen = BTreeSet::new();
    for item in items {
        match load_item(library, item) {
            Ok(Some((reference, problems))) => {
                let key = reference.id.native_key.clone();
                if !seen.insert(key.clone()) {
                    result.warnings.push(LoadWarning {
                        kind: WarningKind::DuplicateKey,
                        line: None,
                        message: format!("the Zotero item `{key}` appears twice; the first copy is used"),
                        key: Some(key),
                    });
                    continue;
                }
                result.warnings.extend(problems.into_iter().map(|message| LoadWarning {
                    kind: WarningKind::Field,
                    line: None,
                    key: Some(key.clone()),
                    message,
                }));
                result.references.push(reference);
            }
            Ok(None) => {}
            Err((key, message)) => result.warnings.push(LoadWarning {
                kind: WarningKind::Syntax,
                line: None,
                key,
                message,
            }),
        }
    }
    let mut owners: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for reference in &result.references {
        if let Some(citekey) = &reference.citekey {
            owners.entry(citekey).or_default().push(&reference.id.native_key);
        }
    }
    let shared: Vec<LoadWarning> = owners
        .into_iter()
        .filter(|(_, items)| items.len() > 1)
        .map(|(citekey, items)| LoadWarning {
            kind: WarningKind::DuplicateKey,
            line: None,
            key: Some(citekey.to_string()),
            message: format!(
                "the citation key `{citekey}` is used by {} items ({}); citing it is ambiguous",
                items.len(),
                items.join(", ")
            ),
        })
        .collect();
    result.warnings.extend(shared);
    result
}

type Loaded = Option<(Reference, Vec<String>)>;

fn load_item(library: Library, item: &Value) -> Result<Loaded, (Option<String>, String)> {
    let key = item
        .get("key")
        .and_then(Value::as_str)
        .filter(|k| !k.is_empty())
        .ok_or((None, "a Zotero item has no item key; it was skipped".to_string()))?;
    let fail = |message: &str| (Some(key.to_string()), message.to_string());
    let data = item
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| fail("the Zotero item has no item data; it was skipped"))?;
    let item_type = data.get("itemType").and_then(Value::as_str).unwrap_or_default();
    if NOT_CITABLE.contains(&item_type) || is_trashed(data) {
        return Ok(None);
    }
    let csl = csl_data(item).ok_or_else(|| fail("the Zotero item has no CSL data; it was skipped"))?;
    let csl = csl.as_ref();

    let mut problems = Vec::new();
    let mut reference = Reference {
        id: RecordId {
            source: library.to_string(),
            native_key: key.to_string(),
        },
        citekey: citation_key(data, csl, &mut problems),
        item_type: match csl.get("type").and_then(Value::as_str).filter(|t| !t.is_empty()) {
            Some(kind) => kind.to_string(),
            None => {
                problems.push("the item has no CSL type; it is treated as a document".to_string());
                "document".to_string()
            }
        },
        title: text(csl, "title").map(|t| rich_text(&t)),
        authors: names(csl, "author", &mut problems),
        editors: names(csl, "editor", &mut problems),
        other_names: BTreeMap::new(),
        issued: None,
        dates: BTreeMap::new(),
        container_title: text(csl, "container-title").map(|t| rich_text(&t)),
        doi: text(csl, "DOI"),
        url: text(csl, "URL"),
        abstract_text: text(csl, "abstract"),
        keywords: tags(data),
        language: text(csl, "language"),
        eprints: Vec::new(),
        variables: BTreeMap::new(),
        raw: serde_json::to_string(item).ok(),
    };
    for (name, value) in csl {
        let name = name.as_str();
        if NAME_FIELDS.contains(&name) || TEXT_FIELDS.contains(&name) || IGNORED.contains(&name) {
            continue;
        }
        if value.is_array() {
            let list = names(csl, name, &mut problems);
            if !list.is_empty() {
                reference.other_names.insert(name.to_string(), list);
            }
        } else if value.get("date-parts").is_some() || value.get("raw").is_some() || value.get("literal").is_some() {
            match date(value, name, &mut problems) {
                Some(date) if name == "issued" => reference.issued = Some(date),
                Some(date) => {
                    reference.dates.insert(name.to_string(), date);
                }
                None => {
                    let shown = value
                        .get("raw")
                        .or_else(|| value.get("literal"))
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    match shown {
                        // Keep a date Zotero could not parse as text, so a
                        // citation processor can still print it.
                        Some(shown) => {
                            reference.variables.insert(name.to_string(), shown);
                        }
                        None => problems.push(format!("the date `{name}` could not be read")),
                    }
                }
            }
        } else if let Some(text) = scalar(value) {
            reference.variables.insert(name.to_string(), text);
        } else if !value.is_null() && value.as_str().is_none() {
            problems.push(format!("the field `{name}` could not be read"));
        }
    }
    Ok(Some((reference, problems)))
}

fn is_trashed(data: &Map<String, Value>) -> bool {
    match data.get("deleted") {
        Some(Value::Bool(deleted)) => *deleted,
        Some(Value::Number(n)) => n.as_u64().is_some_and(|n| n != 0),
        _ => false,
    }
}

/// The item's CSL-JSON. The web API sends it as an object; Zotero's local API
/// sends the text of its CSL JSON export, an array holding this one item.
fn csl_data(item: &Value) -> Option<Cow<'_, Map<String, Value>>> {
    match item.get("csljson")? {
        Value::Object(csl) => Some(Cow::Borrowed(csl)),
        Value::String(text) => match serde_json::from_str(text).ok()? {
            Value::Object(csl) => Some(Cow::Owned(csl)),
            Value::Array(mut items) if items.len() == 1 => match items.pop()? {
                Value::Object(csl) => Some(Cow::Owned(csl)),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

/// Zotero's own citation key field (Zotero 7 and later), then the
/// `Citation Key:` line Better BibTeX and older versions keep in Extra, then
/// the key in the CSL export.
/// A key with spaces or the characters `,`, `{` and `}` would break LaTeX and
/// Pandoc citations, so it is reported and the next source is tried.
fn citation_key(data: &Map<String, Value>, csl: &Map<String, Value>, problems: &mut Vec<String>) -> Option<String> {
    let mut usable = |key: &str| {
        let key = key.trim();
        if key.is_empty() {
            None
        } else if key.chars().any(|c| c.is_whitespace() || matches!(c, ',' | '{' | '}')) {
            problems.push("a citation key contains spaces, commas or braces and was not used".to_string());
            None
        } else {
            Some(key.to_string())
        }
    };
    if let Some(key) = data.get("citationKey").and_then(Value::as_str).and_then(&mut usable) {
        return Some(key);
    }
    let from_extra = data.get("extra").and_then(Value::as_str).and_then(|extra| {
        extra.lines().find_map(|line| {
            let (label, value) = line.split_once(':')?;
            label
                .trim()
                .eq_ignore_ascii_case("citation key")
                .then(|| usable(value))?
        })
    });
    from_extra.or_else(|| csl.get("citation-key").and_then(Value::as_str).and_then(&mut usable))
}

fn text(csl: &Map<String, Value>, name: &str) -> Option<String> {
    csl.get(name).and_then(scalar).filter(|t| !t.is_empty())
}

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn tags(data: &Map<String, Value>) -> Vec<String> {
    data.get("tags")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|tag| tag.get("tag").and_then(Value::as_str))
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_string)
        .collect()
}

fn names(csl: &Map<String, Value>, field: &str, problems: &mut Vec<String>) -> Vec<Name> {
    let Some(list) = csl.get(field) else {
        return Vec::new();
    };
    let Some(list) = list.as_array() else {
        problems.push(format!("the names in `{field}` could not be read"));
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in list {
        let part = |key: &str| {
            entry
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        let name = Name {
            family: part("family"),
            given: part("given"),
            dropping_particle: part("dropping-particle"),
            non_dropping_particle: part("non-dropping-particle"),
            suffix: part("suffix"),
            literal: part("literal"),
        };
        if name == Name::default() {
            problems.push(format!("a name in `{field}` is empty and was left out"));
        } else {
            out.push(name);
        }
    }
    out
}

/// A CSL date with `date-parts`. Seasons (months 13–16), impossible days and
/// an unreadable range end are left out and reported; a date with no valid
/// year is `None`.
fn date(value: &Value, name: &str, problems: &mut Vec<String>) -> Option<Issued> {
    let parts = value.get("date-parts")?.as_array()?;
    let start = partial_date(parts.first()?, name, problems)?;
    let end = match parts.get(1) {
        None => None,
        Some(end) => {
            let end = partial_date(end, name, problems);
            if end.is_none() {
                problems.push(format!(
                    "the end of the date `{name}` could not be read and was left out"
                ));
            }
            end.filter(|end| *end != start)
        }
    };
    let circa = match value.get("circa") {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_u64().is_some_and(|n| n != 0),
        Some(Value::String(s)) => !matches!(s.trim().to_ascii_lowercase().as_str(), "" | "0" | "false"),
        _ => false,
    };
    Some(Issued { start, end, circa })
}

fn partial_date(parts: &Value, name: &str, problems: &mut Vec<String>) -> Option<PartialDate> {
    let parts = parts.as_array()?;
    let number = |i: usize| -> Option<Option<i64>> {
        match parts.get(i)? {
            Value::Number(n) => Some(n.as_i64()),
            Value::String(s) => Some(s.trim().parse().ok()),
            _ => Some(None),
        }
    };
    let year = i32::try_from(number(0)??).ok()?;
    let month = match number(1) {
        None => None,
        Some(m) => {
            let m = m.and_then(|m| u8::try_from(m).ok()).filter(|m| (1..=12).contains(m));
            if m.is_none() {
                problems.push(format!(
                    "the month of the date `{name}` is not a calendar month and was left out"
                ));
            }
            m
        }
    };
    let day = match (month, number(2)) {
        (Some(month), Some(d)) => {
            let d = d
                .and_then(|d| u8::try_from(d).ok())
                .filter(|d| (1..=days_in_month(year, month)).contains(d));
            if d.is_none() {
                problems.push(format!("the day of the date `{name}` does not exist and was left out"));
            }
            d
        }
        _ => None,
    };
    Some(PartialDate { year, month, day })
}

/// Zotero titles are plain text with a few HTML tags for rich text. Keep the
/// tags CSL understands and escape everything else, so a title such as
/// "p < 0.05" is never mistaken for markup.
fn rich_text(title: &str) -> String {
    const TAGS: [&str; 11] = [
        "<i>",
        "</i>",
        "<b>",
        "</b>",
        "<sub>",
        "</sub>",
        "<sup>",
        "</sup>",
        "<span class=\"nocase\">",
        "<span style=\"font-variant:small-caps;\">",
        "</span>",
    ];
    let mut out = String::with_capacity(title.len());
    let mut rest = title;
    while let Some(c) = rest.chars().next() {
        if c == '<'
            && let Some(tag) = TAGS.iter().find(|tag| rest.starts_with(**tag))
        {
            out.push_str(tag);
            rest = &rest[tag.len()..];
            continue;
        }
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
        rest = &rest[c.len_utf8()..];
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn item(key: &str, data: Value, csl: Value) -> Value {
        json!({ "key": key, "version": 7, "data": data, "csljson": csl })
    }

    fn article() -> Value {
        item(
            "ABCD2345",
            json!({
                "key": "ABCD2345",
                "itemType": "journalArticle",
                "citationKey": "garcia2021",
                "tags": [{ "tag": "sleep" }, { "tag": " memory ", "type": 1 }]
            }),
            json!({
                "id": "123/ABCD2345",
                "type": "article-journal",
                "title": "Sleep spindles predict <i>memory</i>",
                "container-title": "Journal of Synthetic Studies",
                "author": [
                    { "family": "García", "given": "Lucía" },
                    { "family": "Okafor", "given": "Chidi", "non-dropping-particle": "van" },
                    { "literal": "Synthetic Consortium" }
                ],
                "translator": [{ "family": "Ito", "given": "Hana" }],
                "issued": { "date-parts": [["2021", 3, 14]] },
                "accessed": { "date-parts": [[2024, 1]] },
                "volume": "12",
                "page": "4-9",
                "number-of-pages": 6,
                "DOI": "10.0000/synthetic.1"
            }),
        )
    }

    #[test]
    fn a_zotero_item_keeps_its_identity_key_and_csl_fields() {
        let result = load_items(Library::Group(42), &[article()]);
        assert_eq!(result.warnings, []);
        let r = &result.references[0];
        assert_eq!(r.id.source, "zotero:group:42");
        assert_eq!(r.id.native_key, "ABCD2345");
        assert_eq!(r.citekey.as_deref(), Some("garcia2021"));
        assert_eq!(r.item_type, "article-journal");
        assert_eq!(r.title.as_deref(), Some("Sleep spindles predict <i>memory</i>"));
        assert_eq!(r.authors.len(), 3);
        assert_eq!(r.authors[1].non_dropping_particle.as_deref(), Some("van"));
        assert_eq!(r.authors[2].literal.as_deref(), Some("Synthetic Consortium"));
        assert_eq!(r.other_names["translator"][0].family.as_deref(), Some("Ito"));
        assert_eq!(
            r.issued,
            Some(Issued {
                start: PartialDate {
                    year: 2021,
                    month: Some(3),
                    day: Some(14)
                },
                end: None,
                circa: false
            })
        );
        assert_eq!(r.dates["accessed"].start.month, Some(1));
        assert_eq!(r.container_title.as_deref(), Some("Journal of Synthetic Studies"));
        assert_eq!(r.doi.as_deref(), Some("10.0000/synthetic.1"));
        assert_eq!(r.keywords, ["sleep", "memory"]);
        assert_eq!(r.variables["page"], "4-9");
        assert_eq!(r.variables["number-of-pages"], "6");
        assert!(!r.variables.contains_key("id"));
        assert_eq!(r.to_csl_json()["id"], "garcia2021");
    }

    #[test]
    fn the_personal_library_has_its_own_source() {
        let r = &load_items(Library::User, &[article()]).references[0];
        assert_eq!(r.id.source, "zotero:user");
    }

    #[test]
    fn csl_json_sent_as_export_text_is_read() {
        // Zotero's local API sends `csljson` as the text of its CSL JSON
        // export: a pretty-printed array holding the one item.
        let Value::Object(mut local) = article() else {
            panic!("the article fixture is an object")
        };
        let csl = local.remove("csljson").unwrap_or_default();
        let text = serde_json::to_string_pretty(&json!([csl])).unwrap_or_default();
        local.insert("csljson".into(), Value::String(text));
        let mut local = load_items(Library::User, &[Value::Object(local)]);
        let mut web = load_items(Library::User, &[article()]);
        assert!(local.warnings.is_empty(), "{:?}", local.warnings);
        assert_eq!(local.references.len(), 1);
        // Every field matches; only the verbatim source text differs.
        for reference in local.references.iter_mut().chain(&mut web.references) {
            reference.raw = None;
        }
        assert_eq!(local.references, web.references);

        for bad in ["not json", "[]", "[1]", "[{}, {}]", "3"] {
            let loaded = load_items(Library::User, &[item("K", json!({ "itemType": "book" }), json!(bad))]);
            assert!(loaded.references.is_empty(), "{bad}");
            assert_eq!(loaded.warnings.len(), 1, "{bad}");
        }
    }

    #[test]
    fn citation_keys_come_from_zotero_then_extra_then_csl() {
        let with = |data: Value, csl: Value| {
            load_items(Library::User, &[item("K", data, csl)]).references[0]
                .citekey
                .clone()
        };
        let csl = json!({ "type": "book", "citation-key": "fromcsl" });
        assert_eq!(
            with(
                json!({ "citationKey": "own", "extra": "Citation Key: extra" }),
                csl.clone()
            ),
            Some("own".into())
        );
        assert_eq!(
            with(
                json!({ "citationKey": "", "extra": "tex.x: y\ncitation key:  extra \n" }),
                csl.clone()
            ),
            Some("extra".into())
        );
        assert_eq!(
            with(json!({ "extra": "Citation Key: two words" }), csl),
            Some("fromcsl".into())
        );
        assert_eq!(with(json!({}), json!({ "type": "book" })), None);
    }

    #[test]
    fn notes_attachments_annotations_and_the_trash_are_not_references() {
        let csl = json!({ "type": "document", "title": "x" });
        let items = [
            item("N", json!({ "itemType": "note" }), csl.clone()),
            item("A", json!({ "itemType": "attachment" }), csl.clone()),
            item("X", json!({ "itemType": "annotation" }), csl.clone()),
            item("T", json!({ "itemType": "book", "deleted": 1 }), csl.clone()),
            item("U", json!({ "itemType": "book", "deleted": true }), csl.clone()),
            item("B", json!({ "itemType": "book", "deleted": false }), csl),
        ];
        let result = load_items(Library::User, &items);
        assert_eq!(result.warnings, []);
        let keys: Vec<_> = result.references.iter().map(|r| r.id.native_key.as_str()).collect();
        assert_eq!(keys, ["B"]);
    }

    #[test]
    fn unreadable_items_and_fields_are_reported_not_dropped_silently() {
        let items = [
            json!({ "data": {}, "csljson": {} }),
            json!({ "key": "NODATA", "csljson": {} }),
            json!({ "key": "NOCSL", "data": { "itemType": "book" } }),
            item(
                "PARTLY",
                json!({ "itemType": "book" }),
                json!({
                    "type": "book",
                    "author": [{}, { "family": "Kept" }],
                    "editor": "not a list",
                    "issued": { "date-parts": [["spring"]] },
                    "event-date": { "raw": "around the solstice" },
                    "note": { "unexpected": true }
                }),
            ),
        ];
        let result = load_items(Library::User, &items);
        let syntax: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.kind == WarningKind::Syntax)
            .map(|w| w.key.clone())
            .collect();
        assert_eq!(syntax, [None, Some("NODATA".into()), Some("NOCSL".into())]);
        let fields: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.kind == WarningKind::Field)
            .map(|w| w.message.as_str())
            .collect();
        assert_eq!(
            fields,
            [
                "a name in `author` is empty and was left out",
                "the names in `editor` could not be read",
                "the date `issued` could not be read",
                "the field `note` could not be read",
            ]
        );
        let r = &result.references[0];
        assert_eq!(r.authors.len(), 1);
        assert_eq!(r.issued, None);
        assert_eq!(r.variables["event-date"], "around the solstice");
    }

    #[test]
    fn dates_keep_ranges_and_uncertainty_but_report_invalid_parts() {
        let parsed = |value: Value| {
            let mut problems = Vec::new();
            (date(&value, "issued", &mut problems), problems)
        };
        assert_eq!(
            parsed(json!({ "date-parts": [[2019, 21], [2020, 2, 31]], "circa": 1 })),
            (
                Some(Issued {
                    start: PartialDate::year(2019),
                    end: Some(PartialDate {
                        year: 2020,
                        month: Some(2),
                        day: None
                    }),
                    circa: true
                }),
                vec![
                    "the month of the date `issued` is not a calendar month and was left out".to_string(),
                    "the day of the date `issued` does not exist and was left out".to_string(),
                ]
            )
        );
        assert_eq!(
            parsed(json!({ "date-parts": [[2020, 2, 29]] })).0.map(|d| d.start.day),
            Some(Some(29))
        );
        assert_eq!(
            parsed(json!({ "date-parts": [[2019], ["x"]] })),
            (
                Some(Issued::year(2019)),
                vec!["the end of the date `issued` could not be read and was left out".to_string()]
            )
        );
        assert_eq!(
            parsed(json!({ "date-parts": [[2019], [2019]] })),
            (Some(Issued::year(2019)), vec![])
        );
        assert_eq!(
            parsed(json!({ "date-parts": [[-50]] })).0.map(|d| d.start.year),
            Some(-50)
        );
        for circa in [json!("false"), json!("0"), json!(""), json!(false), json!(0)] {
            let (d, _) = parsed(json!({ "date-parts": [[2019]], "circa": circa }));
            assert_eq!(d.map(|d| d.circa), Some(false), "{circa}");
        }
        assert_eq!(parsed(json!({ "date-parts": [[]] })).0, None);
        assert_eq!(parsed(json!({ "date-parts": [[99999999999_i64]] })).0, None);
    }

    #[test]
    fn repeated_items_and_shared_citation_keys_are_reported() {
        let with_key =
            |key: &str, citekey: &str| item(key, json!({ "citationKey": citekey }), json!({ "type": "book" }));
        let result = load_items(
            Library::User,
            &[
                with_key("A", "same"),
                with_key("A", "other"),
                with_key("B", "same"),
                with_key("C", "own"),
            ],
        );
        let keys: Vec<_> = result.references.iter().map(|r| r.id.native_key.as_str()).collect();
        assert_eq!(keys, ["A", "B", "C"]);
        let warnings: Vec<_> = result
            .warnings
            .iter()
            .map(|w| (w.kind, w.key.as_deref(), w.message.as_str()))
            .collect();
        assert_eq!(
            warnings,
            [
                (
                    WarningKind::DuplicateKey,
                    Some("A"),
                    "the Zotero item `A` appears twice; the first copy is used"
                ),
                (
                    WarningKind::DuplicateKey,
                    Some("same"),
                    "the citation key `same` is used by 2 items (A, B); citing it is ambiguous"
                ),
            ]
        );
    }

    #[test]
    fn unusable_citation_keys_missing_types_and_empty_fields_are_handled_visibly() {
        let result = load_items(
            Library::User,
            &[item(
                "K",
                json!({ "citationKey": "a,b", "extra": "Citation Key: {x}" }),
                json!({ "citation-key": "good", "volume": "  ", "publisher": "" }),
            )],
        );
        let r = &result.references[0];
        assert_eq!(r.citekey.as_deref(), Some("good"));
        assert_eq!(r.item_type, "document");
        assert!(r.variables.is_empty());
        let messages: Vec<_> = result.warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "a citation key contains spaces, commas or braces and was not used",
                "a citation key contains spaces, commas or braces and was not used",
                "the item has no CSL type; it is treated as a document",
            ]
        );
    }

    #[test]
    fn titles_keep_csl_markup_and_escape_everything_else() {
        assert_eq!(rich_text("p < 0.05 & <b>bold</b>"), "p &lt; 0.05 &amp; <b>bold</b>");
        assert_eq!(
            rich_text("<span class=\"nocase\">iPhone</span> <script>"),
            "<span class=\"nocase\">iPhone</span> &lt;script&gt;"
        );
        assert_eq!(rich_text("Été 🌙 <sub>2</sub>"), "Été 🌙 <sub>2</sub>");
        let r = &load_items(
            Library::User,
            &[item("K", json!({}), json!({ "type": "book", "title": "a < b" }))],
        )
        .references[0];
        assert_eq!(r.title_plain().as_deref(), Some("a < b"));
    }

    #[test]
    fn container_titles_are_rich_text_and_entities_round_trip() {
        let r = &load_items(
            Library::User,
            &[item(
                "K",
                json!({}),
                json!({ "type": "chapter", "title": "Already &amp; escaped &lt;x&gt;", "container-title": "<i>Q</i> & A" }),
            )],
        )
        .references[0];
        assert_eq!(r.container_title.as_deref(), Some("<i>Q</i> &amp; A"));
        assert_eq!(r.title.as_deref(), Some("Already &amp;amp; escaped &amp;lt;x&amp;gt;"));
        assert_eq!(r.title_plain().as_deref(), Some("Already &amp; escaped &lt;x&gt;"));
    }
}
