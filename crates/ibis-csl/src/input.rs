use crate::{FormatError, Limits};
use ibis_core::contract::*;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::io::{self, Write};

pub(crate) struct Prepared {
    pub json: String,
    // Eligibility only: output must prove representation coverage before exposure.
    pub outcomes: Vec<ItemOutcome>,
    pub losses: Vec<Loss>,
    pub notices: Vec<Notice>,
    pub empty: bool,
}

pub(crate) fn prepare(
    snapshot: &Snapshot,
    request: &CitationRequest,
    limits: &Limits,
) -> Result<Prepared, FormatError> {
    if request.items.is_empty() || request.items.len() > 1000 {
        return Err(FormatError::InvalidRequest);
    }
    let OutputSpec::Csl { style } = &request.output else {
        return Err(FormatError::UnsupportedStyle);
    };
    if !crate::STYLES.iter().any(|(id, _)| id == style) {
        return Err(FormatError::UnsupportedStyle);
    }
    if request
        .output_locale
        .as_deref()
        .is_some_and(|locale| !matches!(locale, "en" | "en-US"))
    {
        return Err(FormatError::InvalidLocale);
    }
    bounded_json(request, limits.input_bytes)?;
    if request
        .input_language
        .as_deref()
        .is_some_and(crate::output::unsupported_controls)
        || request.items.iter().any(|pick| {
            crate::output::unsupported_controls(&pick.item.key)
                || pick
                    .annotation
                    .as_ref()
                    .is_some_and(|id| crate::output::unsupported_controls(&id.key))
                || pick
                    .prefix
                    .iter()
                    .chain(&pick.suffix)
                    .any(|run| crate::output::unsupported_controls(&run.text))
        })
    {
        return Err(FormatError::InvalidRequest);
    }

    let bibliography = style == "ieee" || matches!(request.form, CitationForm::BibliographyOnly | CitationForm::Full);
    let mut items = Vec::new();
    let mut cites = Vec::new();
    let mut ids = BTreeMap::<ItemRef, String>::new();
    let mut citable = BTreeMap::<ItemRef, serde_json::Map<String, Value>>::new();
    let mut outcomes = Vec::new();
    let mut losses = Vec::new();
    let mut notices = Vec::new();
    let mut affixes = Vec::new();
    if style == "ieee" {
        notices.extend([Notice::NeedsDocumentContext, Notice::ReferenceEntry]);
    }
    let mut remaining = limits.input_bytes;
    for (index, pick) in request.items.iter().enumerate() {
        let form = pick.form.unwrap_or(request.form);
        let refusal = if (form == CitationForm::AuthorOnly && (pick.suppress_author || request.items.len() > 1))
            // A narrative citation is one sentence's subject: one reference.
            || (form == CitationForm::Narrative && (pick.suppress_author || bibliography || request.items.len() > 1))
            || (style == "ieee" && matches!(form, CitationForm::AuthorOnly | CitationForm::YearOnly))
            // MLA's citation layout has no year; a nonempty locator is not a year.
            || (style == "modern-language-association" && !bibliography && form == CitationForm::YearOnly)
            || ((matches!(form, CitationForm::Full | CitationForm::BibliographyOnly)) != bibliography
                && style != "ieee")
        {
            Some(CitationError::UnsupportedForm)
        } else {
            validate_pick(snapshot, pick)
        };
        if let Some(error) = refusal {
            outcomes.push(ItemOutcome::Refused { item: index, error });
            continue;
        }
        let Some(object) = snapshot.get(&pick.item) else {
            return Err(FormatError::InvalidRequest);
        };
        let LibraryObject::Record(record) = &object.object else {
            return Err(FormatError::InvalidRequest);
        };
        let record = match citable.entry(pick.item.clone()) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let bytes = bounded_json(&record.csl, remaining)?;
                remaining = remaining.saturating_sub(bytes.len());
                let mut value: serde_json::Map<String, Value> =
                    serde_json::from_slice(&bytes).map_err(|_| FormatError::InvalidRequest)?;
                // Notes, annotations and private links never reach the
                // processor; every check below sees what it will print.
                ibis_core::citable::citable_fields(&mut value);
                normalize_strings(value.values_mut());
                if value.keys().any(|key| crate::output::unsupported_controls(key)) || record_controls(value.values()) {
                    return Err(FormatError::InvalidRequest);
                }
                entry.insert(value)
            }
        };
        // Name::OTHERS is source metadata for an incomplete name list, not an
        // institution named "others". citeproc has no data form for that marker.
        // Refuse this item rather than inventing missing authors or printing it.
        // When an item has authors, the in-text layouts of the bundled
        // author-date styles print only these name lists; every other one is
        // a substitute after `author` or appears only in entries.
        let has_authors = named(record.get("author"));
        if record
            .iter()
            .filter(|(field, _)| {
                bibliography
                    || !has_authors
                    || matches!(field.as_str(), "author" | "composer" | "interviewer" | "recipient")
            })
            .filter(|(field, _)| {
                matches!(
                    field.as_str(),
                    "author"
                        | "chair"
                        | "collection-editor"
                        | "compiler"
                        | "composer"
                        | "container-author"
                        | "contributor"
                        | "curator"
                        | "director"
                        | "editor"
                        | "editor-translator"
                        | "editorial-director"
                        | "executive-producer"
                        | "guest"
                        | "host"
                        | "illustrator"
                        | "interviewer"
                        | "narrator"
                        | "organizer"
                        | "original-author"
                        | "performer"
                        | "producer"
                        | "recipient"
                        | "reviewed-author"
                        | "script-writer"
                        | "series-creator"
                        | "translator"
                        | "commenter"
                )
            })
            .filter_map(|(_, value)| value.as_array())
            .any(|names| {
                names
                    .iter()
                    .any(|name| name.get("literal").and_then(Value::as_str) == Some(ibis_core::model::OTHERS))
            })
        {
            outcomes.push(ItemOutcome::Refused {
                item: index,
                error: CitationError::UnsupportedForm,
            });
            continue;
        }
        if matches!(form, CitationForm::AuthorOnly | CitationForm::Narrative) && !has_authors {
            outcomes.push(ItemOutcome::Refused {
                item: index,
                error: CitationError::UnsupportedForm,
            });
            continue;
        }
        let id = if let Some(id) = ids.get(&pick.item) {
            id.clone()
        } else {
            let id = format!("ibis-{}", items.len());
            let mut value = record.clone();
            // Source identities and keys never become JavaScript object lookup keys.
            value.insert("id".into(), json!(id));
            items.push(Value::Object(value));
            ids.insert(pick.item.clone(), id.clone());
            id
        };
        let mut cite = json!({"id":id});
        for (present, field) in [
            (bibliography && !pick.prefix.is_empty(), LossField::Prefix),
            (bibliography && !pick.suffix.is_empty(), LossField::Suffix),
        ] {
            if present {
                losses.push(Loss {
                    item: Some(index),
                    field,
                    reason: LossReason::Unsupported,
                });
            }
        }
        if bibliography {
            for (present, field) in [
                (pick.locator.is_some(), LossField::Locator),
                (
                    pick.suppress_author || matches!(form, CitationForm::AuthorOnly | CitationForm::YearOnly),
                    LossField::AuthorSuppression,
                ),
            ] {
                if present {
                    losses.push(Loss {
                        item: Some(index),
                        field,
                        reason: LossReason::Unsupported,
                    });
                }
            }
        } else {
            if let Some(locator) = &pick.locator {
                if locator.value.trim().is_empty() || locator.value.chars().any(char::is_control) {
                    outcomes.push(ItemOutcome::Refused {
                        item: index,
                        error: CitationError::InvalidLocator,
                    });
                    continue;
                }
                cite["label"] = serde_json::to_value(locator.term).map_err(|_| FormatError::InvalidRequest)?;
                cite["locator"] = json!(locator.value);
            }
            // Rich affixes belong to the 0.4 builder. Omitting them is explicit,
            // never citeproc markup that could reinterpret literal input.
            // citeproc-js would read affix text as markup and curl its quotes, so
            // it sees inert placeholders and the bridge writes each affix in
            // every output format afterwards, with citeproc's own escaping.
            for (key, runs) in [("prefix", &pick.prefix), ("suffix", &pick.suffix)] {
                if runs.iter().any(|run| !run.text.is_empty()) {
                    let placeholder = placeholder(affixes.len(), key, runs);
                    cite[key] = json!(placeholder);
                    affixes.push(json!({"placeholder": placeholder, "kind": key, "runs": runs}));
                }
            }
            cite["suppress-author"] = json!(pick.suppress_author || form == CitationForm::YearOnly);
            cite["author-only"] = json!(form == CitationForm::AuthorOnly);
            if form == CitationForm::Narrative {
                cite["narrative"] = json!(true);
            }
        }
        cites.push(cite);
        outcomes.push(ItemOutcome::Rendered { item: index });
    }
    if style != "ieee" && style != "modern-language-association" && shares_author_and_year(&items) {
        notices.push(Notice::YearSuffixWithheld);
    }
    if bibliography && request.keep_order {
        losses.push(Loss {
            item: None,
            field: LossField::Order,
            reason: LossReason::Unsupported,
        });
    }
    let json=String::from_utf8(bounded_json(&json!({"style":style,"items":items,"cites":cites,"affixes":affixes,"bibliography":bibliography,"keep_order":request.keep_order,"locale":request.output_locale}),limits.input_bytes)?).map_err(|_|FormatError::InvalidRequest)?;
    // Each placeholder (in its cite and in the affix list) must be the only
    // place its sentinel occurs, or the bridge could write an affix into a
    // record's own text.
    if json.matches(PLACEHOLDER_OPEN).count() != 2 * affixes.len()
        || json.matches(PLACEHOLDER_CLOSE).count() != 2 * affixes.len()
    {
        return Err(FormatError::InvalidRequest);
    }
    Ok(Prepared {
        json,
        outcomes,
        losses,
        notices,
        empty: cites.is_empty(),
    })
}

/// Year letters (2026a, 2026b) depend on the document's whole reference list,
/// so the notice appears only when this request already holds two works that
/// may print the same names and year. It errs towards the notice: every name
/// variable counts whatever its role, names compare by particle and family
/// only, and two works without a year count as sharing one.
fn shares_author_and_year(items: &[Value]) -> bool {
    let mut seen = std::collections::BTreeSet::new();
    items.iter().any(|item| {
        let year = year(item.get("issued"));
        let mut signatures: Vec<Vec<String>> = item
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(_, value)| value.as_array())
            .filter(|names| {
                !names.is_empty()
                    && names
                        .iter()
                        .all(|name| name.get("family").or_else(|| name.get("literal")).is_some())
            })
            .map(|names| names.iter().map(name).collect())
            .collect();
        if signatures.is_empty() {
            signatures.push(vec![text(item.get("title"))]);
        }
        signatures.sort();
        signatures.dedup();
        signatures.into_iter().any(|names| !seen.insert((names, year.clone())))
    })
}

fn name(name: &Value) -> String {
    let literal = text(name.get("literal"));
    if !literal.is_empty() {
        return literal;
    }
    [text(name.get("non-dropping-particle")), text(name.get("family"))]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn text(value: Option<&Value>) -> String {
    let value = match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        _ => String::new(),
    };
    value.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// The year as printed, from date parts, a literal or a raw date.
fn year(issued: Option<&Value>) -> Option<String> {
    let issued = issued?;
    let parts = text(issued.pointer("/date-parts/0/0"));
    if !parts.is_empty() {
        return Some(parts);
    }
    ["literal", "raw"].into_iter().find_map(|field| {
        let date = text(issued.get(field));
        let digits: String = date.chars().filter(char::is_ascii_digit).collect();
        (digits.len() == 4)
            .then_some(digits)
            .or_else(|| (!date.is_empty()).then_some(date))
    })
}

fn validate_pick(snapshot: &Snapshot, pick: &CitationItem) -> Option<CitationError> {
    if pick
        .locator
        .as_ref()
        .is_some_and(|locator| locator.value.trim().is_empty() || locator.value.chars().any(char::is_control))
    {
        return Some(CitationError::InvalidLocator);
    }
    let Some(object) = snapshot.get(&pick.item) else {
        return Some(CitationError::MissingItem);
    };
    if !matches!(object.object, LibraryObject::Record(_)) {
        return Some(CitationError::NotCitable);
    }
    if pick
        .expected_cite
        .as_ref()
        .is_some_and(|expected| expected != &object.fingerprints.cite)
    {
        return Some(CitationError::ChangedItem);
    }
    if let Some(id) = &pick.annotation {
        let Some(annotation) = snapshot.get(id) else {
            return Some(CitationError::AnnotationMismatch);
        };
        if pick
            .expected_annotation_cite
            .as_ref()
            .is_some_and(|expected| expected != &annotation.fingerprints.cite)
        {
            return Some(CitationError::AnnotationMismatch);
        }
        let LibraryObject::Annotation(annotation) = &annotation.object else {
            return Some(CitationError::AnnotationMismatch);
        };
        let Some(attachment) = snapshot.get(&annotation.parent) else {
            return Some(CitationError::AnnotationMismatch);
        };
        if !matches!(&attachment.object,LibraryObject::Attachment(attachment) if attachment.parent.as_ref()==Some(&pick.item))
        {
            return Some(CitationError::AnnotationMismatch);
        }
    } else if pick.expected_annotation_cite.is_some() {
        return Some(CitationError::AnnotationMismatch);
    }
    None
}

fn record_controls<'a>(values: impl Iterator<Item = &'a Value>) -> bool {
    let mut pending: Vec<_> = values.collect();
    while let Some(value) = pending.pop() {
        match value {
            Value::String(text) if crate::output::unsupported_controls(text) => return true,
            Value::Object(map) => {
                if map.keys().any(|key| crate::output::unsupported_controls(key)) {
                    return true;
                }
                pending.extend(map.values());
            }
            Value::Array(values) => pending.extend(values),
            _ => {}
        }
    }
    false
}

// citeproc's title casing can silently discard words at line boundaries. CSL
// fields are inline content: preserve their words before the processor sees them.
fn normalize_strings<'a>(values: impl Iterator<Item = &'a mut Value>) {
    let mut pending: Vec<_> = values.collect();
    while let Some(value) = pending.pop() {
        match value {
            Value::String(text) => {
                let mut normalized = String::with_capacity(text.len());
                let mut line_break = false;
                for ch in text.chars() {
                    let is_break = matches!(ch, '\r' | '\n' | '\u{85}' | '\u{2028}' | '\u{2029}');
                    if is_break {
                        // A break between words is a space; one at either
                        // end is nothing, or the processor prints it.
                        if !line_break && !normalized.is_empty() {
                            normalized.push(' ');
                        }
                    } else {
                        normalized.push(if ch.is_control() && ch != '\t' { ' ' } else { ch });
                    }
                    line_break = is_break;
                }
                if line_break && normalized.ends_with(' ') {
                    normalized.pop();
                }
                *text = normalized;
            }
            Value::Object(map) => pending.extend(map.values_mut()),
            Value::Array(values) => pending.extend(values),
            _ => {}
        }
    }
}

/// Abort serialization at the boundary instead of allocating an unbounded JSON copy.
pub(crate) fn bounded_json<T: Serialize>(value: &T, limit: usize) -> Result<Vec<u8>, FormatError> {
    struct Bounded {
        bytes: Vec<u8>,
        limit: usize,
    }
    impl Write for Bounded {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if buf.len() > self.limit.saturating_sub(self.bytes.len()) {
                return Err(io::Error::other("format input limit"));
            }
            self.bytes.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Bounded {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut writer, value).map_err(|_| FormatError::ResourceLimit)?;
    Ok(writer.bytes)
}

/// A name list with at least one name that prints.
fn named(names: Option<&Value>) -> bool {
    names.and_then(Value::as_array).is_some_and(|names| {
        names.iter().any(|name| {
            ["literal", "family", "given"].into_iter().any(|field| {
                name.get(field)
                    .and_then(Value::as_str)
                    .is_some_and(|text| !text.trim().is_empty())
            })
        })
    })
}

/// Sentinels for affix placeholders. Requests whose text contains them are
/// refused, so a placeholder can only ever match itself.
pub(crate) const PLACEHOLDER_OPEN: char = '\u{E000}';
pub(crate) const PLACEHOLDER_CLOSE: char = '\u{E001}';

/// An inert stand-in for one affix. citeproc-js decides delimiter spacing from
/// a suffix's last and a prefix's first character when it is `;`, `.` or `,`,
/// so that character stays visible to it and is part of what gets replaced.
fn placeholder(index: usize, kind: &str, runs: &[TextRun]) -> String {
    let digits: String = index
        .to_string()
        .chars()
        .map(|d| char::from_u32(0xE010 + d.to_digit(10).unwrap_or(0)).unwrap_or(PLACEHOLDER_CLOSE))
        .collect();
    let token = format!("{PLACEHOLDER_OPEN}{digits}{PLACEHOLDER_CLOSE}");
    let text: String = runs.iter().map(|run| run.text.as_str()).collect();
    let edge = if kind == "prefix" {
        text.chars().next()
    } else {
        text.chars().next_back()
    };
    match edge.filter(|c| matches!(c, ';' | '.' | ',')) {
        Some(c) if kind == "prefix" => format!("{c}{token}"),
        Some(c) => format!("{token}{c}"),
        None => token,
    }
}
