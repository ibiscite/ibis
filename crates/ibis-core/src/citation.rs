//! Pure citation eligibility, shared by renderers and delivery rechecks.
use crate::contract::*;
use crate::syntax::{MAX_CITATION_BYTES, MAX_CITATION_ITEMS};

/// Output-specific capabilities. A renderer supplies these without changing
/// the precedence of library, annotation, locator and form refusals.
pub struct ItemSupport {
    pub locator: bool,
    pub form: bool,
    /// The output cites, so a record needs a title, author or editor to be
    /// cited by (decision 0061). Record exports (BibTeX, CSL-JSON, DOI
    /// links) carry whatever a record has and do not ask.
    pub details: bool,
}

impl ItemSupport {
    /// Everything allowed: a recheck of what a renderer already accepted.
    pub const ALL: Self = Self {
        locator: true,
        form: true,
        details: false,
    };
}

/// Eligibility of one item; the record is borrowed from the current snapshot.
pub type ItemCheck<'a> = Result<&'a Record, CitationError>;

/// Enforce the citation item limit before resolving keys, records or references.
pub fn check_item_limit(count: usize) -> Result<(), CitationError> {
    if count > MAX_CITATION_ITEMS {
        return Err(CitationError::TooManyItems);
    }
    Ok(())
}

/// One eligibility outcome per input item, indexed in input order. An eligible
/// record must still be represented by the renderer before delivery.
/// Whole-request refusals are returned separately, before inspecting items.
/// `shown` binds identity and annotation file meaning to an earlier preview,
/// even when the resulting text would be identical.
pub fn check<'a>(
    now: &'a Snapshot,
    shown: Option<&Snapshot>,
    request: &CitationRequest,
    form_support: impl Fn(&CitationItem) -> ItemSupport,
) -> Result<Vec<ItemCheck<'a>>, CitationError> {
    validate_request(request)?;
    Ok(request
        .items
        .iter()
        .map(|pick| {
            let object = now.get(&pick.item).ok_or(CitationError::MissingItem)?;
            if pick
                .expected_cite
                .as_ref()
                .is_some_and(|expected| expected != &object.fingerprints.cite)
                || shown.is_some_and(|old| {
                    old.get(&pick.item)
                        .is_none_or(|old| old.fingerprints.cite != object.fingerprints.cite)
                })
            {
                return Err(CitationError::ChangedItem);
            }
            let LibraryObject::Record(record) = &object.object else {
                return Err(CitationError::NotCitable);
            };
            validate_annotation(now, shown, pick)?;
            let support = form_support(pick);
            if pick
                .locator
                .as_ref()
                .is_some_and(|locator| locator.value.trim().is_empty() || locator.value.chars().any(char::is_control))
                || !support.locator
            {
                return Err(CitationError::InvalidLocator);
            }
            if !support.form {
                return Err(CitationError::UnsupportedForm);
            }
            // Nothing to cite it by: never filled in from elsewhere (decision 0061).
            if support.details && !has_details(record) {
                return Err(CitationError::MissingDetails);
            }
            Ok(record)
        })
        .collect())
}

/// Whether a record has a title, an author or an editor to be cited by, in
/// the fields a citation actually uses (note overrides applied).
pub fn has_details(record: &Record) -> bool {
    let fields = crate::citable::CitableFields::new(record.csl.to_map());
    let text = |part: &Option<String>| part.as_deref().is_some_and(|text| !text.trim().is_empty());
    fields
        .plain()
        .get("title")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|title| !title.trim().is_empty())
        || ["author", "editor"].iter().any(|role| {
            fields
                .names(role)
                .iter()
                .any(|name| text(&name.family) || text(&name.given) || text(&name.literal))
        })
}

fn validate_annotation(now: &Snapshot, shown: Option<&Snapshot>, pick: &CitationItem) -> Result<(), CitationError> {
    let invalid = CitationError::AnnotationMismatch;
    let Some(id) = &pick.annotation else {
        return if pick.expected_annotation_cite.is_some() {
            Err(invalid)
        } else {
            Ok(())
        };
    };
    let object = now.get(id).ok_or(invalid)?;
    if pick
        .expected_annotation_cite
        .as_ref()
        .is_some_and(|expected| expected != &object.fingerprints.cite)
        || shown.is_some_and(|old| {
            old.get(id)
                .is_none_or(|old| old.fingerprints.cite != object.fingerprints.cite)
        })
    {
        return Err(invalid);
    }
    let LibraryObject::Annotation(annotation) = &object.object else {
        return Err(invalid);
    };
    let parent = now.get(&annotation.parent).ok_or(invalid)?;
    let LibraryObject::Attachment(attachment) = &parent.object else {
        return Err(invalid);
    };
    if attachment.parent.as_ref() != Some(&pick.item)
        || shown.is_some_and(|old| {
            old.get(&annotation.parent)
                .is_none_or(|old| old.fingerprints.cite != parent.fingerprints.cite)
        })
    {
        return Err(invalid);
    }
    match (&annotation.page_label, &pick.locator) {
        (Some(page), Some(locator)) if locator.term == LocatorTerm::Page && page.trim() == locator.value.trim() => {
            Ok(())
        }
        _ => Err(invalid),
    }
}

fn validate_request(request: &CitationRequest) -> Result<(), CitationError> {
    if request.items.is_empty() {
        return Err(CitationError::EmptySelection);
    }
    check_item_limit(request.items.len())?;
    for locale in [request.input_language.as_deref(), request.output_locale.as_deref()]
        .into_iter()
        .flatten()
    {
        if locale.len() > 63
            || locale.is_empty()
            || !locale.split('-').enumerate().all(|(index, part)| {
                !part.is_empty()
                    && part.len() <= 8
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_alphabetic() || index > 0 && byte.is_ascii_digit())
            })
        {
            return Err(CitationError::InvalidLocale);
        }
    }
    let mut bytes = 0_usize;
    for item in &request.items {
        bytes = bytes.saturating_add(item.locator.as_ref().map_or(0, |locator| locator.value.len()));
        if item.prefix.len().saturating_add(item.suffix.len()) > 10_000 {
            return Err(CitationError::InvalidRichText);
        }
        for run in item.prefix.iter().chain(&item.suffix) {
            bytes = bytes.saturating_add(run.text.len()).saturating_add(16);
        }
        if bytes > MAX_CITATION_BYTES {
            return Err(CitationError::InvalidRichText);
        }
    }
    Ok(())
}
