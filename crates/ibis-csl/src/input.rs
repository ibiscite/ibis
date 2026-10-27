use crate::{FormatError, Limits};
use ibis_core::contract::*;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
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
    let mut outcomes = Vec::new();
    let mut losses = Vec::new();
    let mut notices = Vec::new();
    if style == "ieee" {
        notices.extend([Notice::NeedsDocumentContext, Notice::ReferenceEntry]);
    }
    let mut remaining = limits.input_bytes;
    for (index, pick) in request.items.iter().enumerate() {
        let form = pick.form.unwrap_or(request.form);
        let refusal = if (form == CitationForm::AuthorOnly && (pick.suppress_author || request.items.len() > 1))
            || form == CitationForm::Narrative
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
        // Name::OTHERS is source metadata for an incomplete name list, not an
        // institution named "others". citeproc has no data form for that marker.
        // Refuse this item rather than inventing missing authors or printing it.
        if record
            .csl
            .iter()
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
        if form == CitationForm::AuthorOnly
            && !record.csl.get("author").and_then(Value::as_array).is_some_and(|names| {
                names.iter().any(|name| {
                    ["literal", "family", "given"].into_iter().any(|field| {
                        name.get(field)
                            .and_then(Value::as_str)
                            .is_some_and(|text| !text.trim().is_empty())
                    })
                })
            })
        {
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
            let bytes = bounded_json(&record.csl, remaining)?;
            remaining = remaining.saturating_sub(bytes.len());
            let mut value: serde_json::Map<String, Value> =
                serde_json::from_slice(&bytes).map_err(|_| FormatError::InvalidRequest)?;
            normalize_strings(value.values_mut());
            if value.keys().any(|key| crate::output::unsupported_controls(key)) || record_controls(value.values()) {
                return Err(FormatError::InvalidRequest);
            }
            // Source identities and keys never become JavaScript object lookup keys.
            value.insert("id".into(), json!(id));
            items.push(Value::Object(value));
            ids.insert(pick.item.clone(), id.clone());
            id
        };
        let mut cite = json!({"id":id});
        for (present, field) in [
            (!pick.prefix.is_empty(), LossField::Prefix),
            (!pick.suffix.is_empty(), LossField::Suffix),
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
            cite["prefix"] = json!("");
            cite["suffix"] = json!("");
            cite["suppress-author"] = json!(pick.suppress_author || form == CitationForm::YearOnly);
            cite["author-only"] = json!(form == CitationForm::AuthorOnly);
        }
        cites.push(cite);
        outcomes.push(ItemOutcome::Rendered { item: index });
    }
    if style != "ieee" && style != "modern-language-association" {
        notices.push(Notice::YearSuffixWithheld);
    }
    if bibliography && request.keep_order {
        losses.push(Loss {
            item: None,
            field: LossField::Order,
            reason: LossReason::Unsupported,
        });
    }
    let json=String::from_utf8(bounded_json(&json!({"style":style,"items":items,"cites":cites,"bibliography":bibliography,"keep_order":request.keep_order,"locale":request.output_locale}),limits.input_bytes)?).map_err(|_|FormatError::InvalidRequest)?;
    Ok(Prepared {
        json,
        outcomes,
        losses,
        notices,
        empty: cites.is_empty(),
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
                        if !line_break {
                            normalized.push(' ');
                        }
                    } else {
                        normalized.push(if ch.is_control() && ch != '\t' { ' ' } else { ch });
                    }
                    line_break = is_break;
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
