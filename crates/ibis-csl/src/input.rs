use crate::{FormatError, Limits};
use ibis_core::contract::*;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::io::{self, Write};
use std::sync::LazyLock;

#[derive(Clone, Copy, Default)]
struct StyleTraits {
    numeric: bool,
    citation_has_issued: bool,
}

static STYLE_TRAITS: LazyLock<BTreeMap<&str, StyleTraits>> = LazyLock::new(|| {
    crate::runtime::style_xml()
        .iter()
        .map(|(id, xml)| (*id, style_traits(xml)))
        .collect()
});

// Read only trusted embedded CSL tags, preserving quoted attributes and skipping
// comments. This is a capability scan, not a validator or an external XML loader.
fn style_traits(mut xml: &str) -> StyleTraits {
    let mut traits = StyleTraits::default();
    let mut stack = Vec::<(&str, Option<&str>)>::new();
    let mut dependencies = BTreeMap::<&str, Vec<&str>>::new();
    let mut issued = Vec::new();
    while let Some(start) = xml.find('<') {
        xml = &xml[start + 1..];
        if let Some(comment) = xml.strip_prefix("!--") {
            xml = comment.split_once("-->").map_or("", |(_, rest)| rest);
            continue;
        }
        let mut quote = None;
        let Some(end) = xml.find(|ch| {
            if quote == Some(ch) {
                quote = None;
            } else if quote.is_none() && matches!(ch, '\'' | '"') {
                quote = Some(ch);
            }
            ch == '>' && quote.is_none()
        }) else {
            break;
        };
        let tag = xml[..end].trim();
        xml = &xml[end + 1..];
        if tag.starts_with(['?', '!']) {
            continue;
        }
        if tag.starts_with('/') {
            stack.pop();
            continue;
        }
        let self_closing = tag.ends_with('/');
        let tag = tag.trim_end_matches('/').trim_end();
        let name_end = tag.find(char::is_whitespace).unwrap_or(tag.len());
        let (name, attrs) = tag.split_at(name_end);
        let parent = stack.last().copied();
        let mut scope = parent.and_then(|(_, scope)| scope);
        if name == "category" && parent.is_some_and(|(name, _)| name == "info") {
            // Labels need not be document citation numbers; only numeric falls back.
            traits.numeric |= trait_attribute(attrs, "citation-format").as_deref() == Some("numeric");
        }
        if name == "macro" {
            scope = attribute(attrs, "name");
        } else if name == "layout" && parent.is_some_and(|(name, _)| name == "citation") {
            scope = Some(""); // Citation layout, excluding its sort keys.
        }
        if let Some(scope) = scope {
            if matches!(name, "date" | "text") && trait_attribute(attrs, "variable").as_deref() == Some("issued") {
                issued.push(scope);
            }
            if let Some(called) = attribute(attrs, "macro") {
                dependencies.entry(scope).or_default().push(called);
            }
        }
        if !self_closing {
            stack.push((name, scope));
        }
    }
    let mut pending = vec![""];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(scope) = pending.pop() {
        if seen.insert(scope) {
            traits.citation_has_issued |= issued.contains(&scope);
            pending.extend(dependencies.get(scope).into_iter().flatten().copied());
        }
    }
    traits
}

fn attribute<'a>(mut attrs: &'a str, key: &str) -> Option<&'a str> {
    while let Some((name, rest)) = attrs.trim_start().split_once('=') {
        let rest = rest.trim_start();
        let quote = rest.chars().next()?;
        if !matches!(quote, '\'' | '"') {
            return None;
        }
        let (value, rest) = rest[1..].split_once(quote)?;
        if name.trim_end() == key {
            return Some(value);
        }
        attrs = rest;
    }
    None
}

// These ASCII trait values cannot contain named XML entities, but may use
// decimal or hexadecimal character references. Decode those exactly once.
fn trait_attribute(attrs: &str, key: &str) -> Option<String> {
    let mut value = attribute(attrs, key)?;
    let mut decoded = String::new();
    while let Some(start) = value.find("&#") {
        decoded.push_str(&value[..start]);
        let (entity, rest) = value[start + 2..].split_once(';')?;
        let code = entity
            .strip_prefix('x')
            .map_or_else(|| entity.parse(), |hex| u32::from_str_radix(hex, 16));
        decoded.push(code.ok().and_then(char::from_u32)?);
        value = rest;
    }
    decoded.push_str(value);
    Some(decoded)
}

pub(crate) struct Prepared {
    pub json: String,
    // Eligibility only: output must prove representation coverage before exposure.
    pub outcomes: Vec<ItemOutcome>,
    pub losses: Vec<Loss>,
    pub notices: Vec<Notice>,
    pub warnings: Vec<CitationWarning>,
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
    let traits = STYLE_TRAITS.get(style.as_str()).ok_or(FormatError::UnsupportedStyle)?;
    prepare_with_traits(snapshot, request, limits, *traits)
}

fn prepare_with_traits(
    snapshot: &Snapshot,
    request: &CitationRequest,
    limits: &Limits,
    traits: StyleTraits,
) -> Result<Prepared, FormatError> {
    let OutputSpec::Csl { style } = &request.output else {
        return Err(FormatError::UnsupportedStyle);
    };
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

    let bibliography = traits.numeric || matches!(request.form, CitationForm::BibliographyOnly | CitationForm::Full);
    let mut items = Vec::new();
    let mut cites = Vec::new();
    let mut ids = BTreeMap::<ItemRef, String>::new();
    let mut citable = BTreeMap::<ItemRef, serde_json::Map<String, Value>>::new();
    let mut outcomes = Vec::new();
    let mut losses = Vec::new();
    let mut notices = Vec::new();
    let mut warnings = Vec::new();
    let mut affixes = Vec::new();
    if traits.numeric {
        notices.extend([Notice::NeedsDocumentContext, Notice::ReferenceEntry]);
    }
    let mut remaining = limits.input_bytes;
    for (index, pick) in request.items.iter().enumerate() {
        let form = pick.form.unwrap_or(request.form);
        let refusal = if (form == CitationForm::AuthorOnly && (pick.suppress_author || request.items.len() > 1))
            // A narrative citation is one sentence's subject: one reference.
            || (form == CitationForm::Narrative && (pick.suppress_author || bibliography || request.items.len() > 1))
            || (traits.numeric && matches!(form, CitationForm::AuthorOnly | CitationForm::YearOnly))
            // A nonempty locator cannot stand in for an absent publication year.
            || (!traits.citation_has_issued && !bibliography && form == CitationForm::YearOnly)
            || ((matches!(form, CitationForm::Full | CitationForm::BibliographyOnly)) != bibliography
                && !traits.numeric)
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
        warnings.extend(record.warnings.iter().map(|code| CitationWarning {
            item: index,
            code: *code,
        }));
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
                normalize_strings(value.values_mut())?;
                if value.keys().any(|key| crate::output::unsupported_controls(key)) || record_controls(value.values()) {
                    return Err(FormatError::InvalidRequest);
                }
                entry.insert(value)
            }
        };
        if style == "apa"
            && bibliography
            && record.get("title").is_some()
            && record.get("language").and_then(Value::as_str).is_none_or(|language| {
                language
                    .split('-')
                    .next()
                    .is_some_and(|primary| primary.eq_ignore_ascii_case("en"))
            })
        {
            warnings.push(CitationWarning {
                item: index,
                code: CitationWarningCode::SentenceCaseExpected,
            });
        }
        // Name::OTHERS is source metadata for an incomplete name list, not an
        // institution named "others". citeproc has no data form for that marker.
        // Refuse this item rather than inventing missing authors or printing it.
        // APA's parenthetical names use a symbol conjunction that cannot be
        // reused as a narrative subject. Refuse multiple names until the
        // narrative path has its own verified name rendering.
        let apa_narrative = style == "apa" && form == CitationForm::Narrative;
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
                    || apa_narrative && names.len() > 1
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
            // Private sort metadata never changes the canonical source date or
            // its rendering. The same year-letter parser owns source detection
            // and this numeric sort projection; source custom fields were removed.
            let sorts = date_sorts(record);
            if !sorts.is_empty() {
                value.insert("ibis-date-sort".into(), Value::Object(sorts));
            }
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
                cite["locator"] = json!(
                    locator
                        .value
                        .chars()
                        .filter(|ch| !bidi_control(*ch))
                        .collect::<String>()
                );
            }
            // Rich affixes belong to the 0.4 builder. Omitting them is explicit,
            // never citeproc markup that could reinterpret literal input.
            // citeproc-js would read affix text as markup and curl its quotes, so
            // it sees inert placeholders and the bridge writes each affix in
            // every output format afterwards, with citeproc's own escaping.
            for (key, runs) in [("prefix", &pick.prefix), ("suffix", &pick.suffix)] {
                let mut runs = runs.clone();
                for run in &mut runs {
                    run.text.retain(|ch| !bidi_control(ch));
                }
                if runs.iter().any(|run| !run.text.is_empty()) {
                    let placeholder = placeholder(affixes.len(), key, &runs);
                    cite[key] = json!(placeholder);
                    affixes.push(json!({"placeholder": placeholder, "kind": key, "runs": runs}));
                }
            }
            cite["suppress-author"] = json!(pick.suppress_author || form == CitationForm::YearOnly);
            cite["author-only"] = json!(form == CitationForm::AuthorOnly);
            if form == CitationForm::YearOnly {
                cite["year-only"] = json!(true);
            }
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
    let json=String::from_utf8(bounded_json(&json!({"style":style,"numeric":traits.numeric,"items":items,"cites":cites,"affixes":affixes,"bibliography":bibliography,"keep_order":request.keep_order,"locale":request.output_locale}),limits.input_bytes)?).map_err(|_|FormatError::InvalidRequest)?;
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
        warnings,
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
    let literal = text(issued.get("literal"));
    if !literal.is_empty() {
        return Some(literal);
    }
    let date = text(issued.get("raw"));
    let digits: String = date.chars().filter(char::is_ascii_digit).collect();
    (digits.len() == 4)
        .then_some(digits)
        .or_else(|| (!date.is_empty()).then_some(date))
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
fn normalize_strings<'a>(values: impl Iterator<Item = &'a mut Value>) -> Result<(), FormatError> {
    let mut pending: Vec<_> = values.collect();
    while let Some(value) = pending.pop() {
        match value {
            Value::String(text) => {
                let mut normalized = String::with_capacity(text.len());
                let mut line_break = false;
                let mut inside_tag = false;
                for ch in text.chars() {
                    // Directional formatting must not escape a field and
                    // reorder the citation around it. Keep RTL letters and
                    // shaping characters (including ZWJ/ZWNJ) unchanged.
                    if bidi_control(ch) {
                        // Removing a control inside tag-looking text could
                        // create markup that the source never contained.
                        if inside_tag {
                            return Err(FormatError::InvalidRequest);
                        }
                        continue;
                    }
                    match ch {
                        '<' => inside_tag = true,
                        '>' => inside_tag = false,
                        _ => {}
                    }
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
    Ok(())
}

fn bidi_control(ch: char) -> bool {
    matches!(ch, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{206f}')
}

fn date_sorts(fields: &serde_json::Map<String, Value>) -> serde_json::Map<String, Value> {
    fields
        .iter()
        .filter_map(|(field, date)| {
            if !matches!(
                field.as_str(),
                "issued"
                    | "event-date"
                    | "accessed"
                    | "original-date"
                    | "publication-date"
                    | "available-date"
                    | "submitted"
                    | "container"
            ) || date
                .get("date-parts")
                .is_some_and(|parts| !parts.as_array().is_some_and(Vec::is_empty))
            {
                return None;
            }
            let literal = date.get("literal").and_then(Value::as_str).or_else(|| {
                // A quoted raw override is a literal to the pinned date parser.
                let raw = date.get("raw")?.as_str()?.trim();
                raw.strip_prefix('"').and_then(|s| s.strip_suffix('"'))
            })?;
            let (year, suffix) = ibis_core::citable::literal_year(literal)?;
            (year != 0).then(|| (field.clone(), json!({"year":year,"suffix":suffix.to_ascii_lowercase()})))
        })
        .collect()
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use CitationForm::{AuthorOnly, Full, Narrative, Normal, YearOnly};

    #[test]
    fn xml_traits_drive_fallback_and_form_refusals_independently_of_style_name() {
        let id = ItemRef {
            library: LibraryId::file(b"synthetic-traits"),
            key: "work".into(),
        };
        let csl = json!({"type":"book","author":[{"family":"Taylor"}],"issued":{"date-parts":[[2026]]}});
        let record = serde_json::from_value(json!({"id":id,"csl":csl})).unwrap();
        let snapshot = Snapshot::new(vec![LibraryObject::Record(record)]).unwrap();
        for (name, format, numeric) in [
            ("invented-numbered", "numeric", true),
            ("invented-escaped", "nu&#109;eric", true),
            ("ieee", "author-date", false),
            ("invented-label", "label", false),
        ] {
            let xml = format!(
                "<style><info><title>{name}</title><category citation-format = '{format}'/></info><citation><layout><date variable='issued'/></layout></citation></style>"
            );
            let traits = style_traits(&xml);
            let mut request: CitationRequest =
                serde_json::from_value(json!({"items":[{"item":id}],"output":{"kind":"csl","style":name}})).unwrap();
            for form in [Normal, Full, AuthorOnly, YearOnly, Narrative] {
                request.form = form;
                let prepared = prepare_with_traits(&snapshot, &request, &Limits::default(), traits).unwrap();
                let json: Value = serde_json::from_str(&prepared.json).unwrap();
                assert_eq!(json["numeric"], numeric);
                assert_eq!(json["bibliography"], numeric || form == Full);
                assert_eq!(prepared.notices.contains(&Notice::NeedsDocumentContext), numeric);
                assert_eq!(prepared.notices.contains(&Notice::ReferenceEntry), numeric);
                assert_eq!(
                    prepared.empty,
                    numeric && matches!(form, AuthorOnly | YearOnly | Narrative)
                );
                if !numeric && form == Narrative {
                    assert_eq!(json["cites"][0]["narrative"], true);
                }
                if !numeric && form == YearOnly {
                    assert_eq!(json["cites"][0]["year-only"], true);
                }
            }
        }
        let traits = style_traits(
            "<style><macro name='unused'><date variable='issued'/></macro><citation><sort><key macro='unused'/></sort><layout><text variable='locator'/></layout></citation><bibliography><layout><date variable='issued'/></layout></bibliography></style>",
        );
        let request = serde_json::from_value(json!({"items":[{"item":id,"locator":{"term":"page","value":"42"}}],"form":"year-only","output":{"kind":"csl","style":"invented-no-year"}})).unwrap();
        let prepared = prepare_with_traits(&snapshot, &request, &Limits::default(), traits).unwrap();
        let refused = json!([{"outcome":"refused","item":0,"error":"unsupported-form"}]);
        assert_eq!(serde_json::to_value(&prepared.outcomes).unwrap(), refused);
    }

    #[test]
    fn bundled_styles_keep_the_behaviour_their_names_used_to_choose() {
        let traits: Vec<_> = STYLE_TRAITS
            .iter()
            .map(|(id, traits)| (*id, traits.numeric, traits.citation_has_issued))
            .collect();
        assert_eq!(
            traits,
            [
                ("apa", false, true),
                ("chicago-author-date", false, true),
                ("harvard-cite-them-right", false, true),
                ("ieee", true, false),
                ("modern-language-association", false, false),
            ]
        );
    }

    #[test]
    fn issued_scan_follows_only_reachable_macros_and_terminates_cycles() {
        for (layout, expected) in [
            ("<text macro='outer'/>", true),
            ("<date variable='issued'/>", true),
            ("<if variable='issued'><text variable='locator'/></if>", false),
            ("<text macro='cycle'/>", false),
        ] {
            let xml = format!(
                "<style><!-- <category citation-format='numeric'/> --><macro name='outer'><text macro='inner'/></macro><macro name='inner'><choose><if type='book'><date variable='issued' date-parts='year'/></if></choose></macro><macro name='cycle'><text macro='cycle'/></macro><citation><layout prefix='>'>{layout}</layout></citation></style>"
            );
            let traits = style_traits(&xml);
            assert!(!traits.numeric);
            assert_eq!(traits.citation_has_issued, expected);
        }
    }
}
