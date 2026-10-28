//! Key outputs over the shared citation contract. No front end resolves keys,
//! chooses a fallback form, or constructs clipboard text.

use std::collections::HashMap;

use crate::contract::*;

pub(crate) mod escape;
pub(crate) mod read;
mod render;

pub const MAX_CITATION_ITEMS: usize = 1_000;
pub const MAX_CITATION_BYTES: usize = 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldSupport {
    Native,
    /// Text is retained outside the citation rather than as a native field.
    Flattened,
    Unsupported,
    SingleItem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Escaping {
    Pandoc,
    Latex,
    Typst,
    BareKeys,
    Json,
    /// Org mode has no escape character: text that Org would read as markup is
    /// not carried, with a typed loss.
    Org,
    Myst,
    MultiMarkdown,
    AsciiDoc,
    Html,
    Liquid,
    CaywJson,
}

/// This table drives validation and field losses, rather than merely describing
/// the implementation. `SingleItem` never silently degrades a multi-item form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntaxCapabilities {
    pub syntax: KeySyntax,
    pub command: Option<&'static str>,
    pub forms: &'static [CitationForm],
    pub single_item_forms: &'static [CitationForm],
    pub locator: FieldSupport,
    /// Prefix placement depends on the citation form as well as the syntax.
    pub prefix: &'static [(CitationForm, FieldSupport)],
    pub suffix: FieldSupport,
    pub rich_text: FieldSupport,
    pub author_suppression: FieldSupport,
    pub escaping: Escaping,
}

impl SyntaxCapabilities {
    pub fn prefix_support(&self, form: CitationForm) -> FieldSupport {
        self.prefix
            .iter()
            .find_map(|(candidate, support)| (*candidate == form).then_some(*support))
            .unwrap_or(FieldSupport::Unsupported)
    }
}

use CitationForm::{Narrative, Normal, YearOnly};
use FieldSupport::{Flattened, Native, SingleItem, Unsupported};

pub const CAPABILITIES: [SyntaxCapabilities; 16] = [
    SyntaxCapabilities {
        syntax: KeySyntax::Pandoc,
        command: None,
        forms: &[Normal, Narrative, YearOnly],
        single_item_forms: &[Narrative],
        locator: Native,
        prefix: &[(Normal, Native), (Narrative, Flattened), (YearOnly, Native)],
        suffix: Native,
        rich_text: Native,
        author_suppression: Native,
        escaping: Escaping::Pandoc,
    },
    SyntaxCapabilities {
        syntax: KeySyntax::Biblatex,
        command: Some("autocite"),
        forms: &[Normal, YearOnly],
        single_item_forms: &[YearOnly],
        locator: Native,
        prefix: &[(Normal, Native), (YearOnly, Native)],
        suffix: Native,
        rich_text: Native,
        author_suppression: SingleItem,
        escaping: Escaping::Latex,
    },
    SyntaxCapabilities {
        syntax: KeySyntax::Biblatex,
        command: Some("parencite"),
        forms: &[Normal, YearOnly],
        single_item_forms: &[YearOnly],
        locator: Native,
        prefix: &[(Normal, Native), (YearOnly, Native)],
        suffix: Native,
        rich_text: Native,
        author_suppression: SingleItem,
        escaping: Escaping::Latex,
    },
    SyntaxCapabilities {
        syntax: KeySyntax::Biblatex,
        command: Some("textcite"),
        forms: &[Normal, Narrative],
        single_item_forms: &[],
        locator: Native,
        prefix: &[(Normal, Native), (Narrative, Native)],
        suffix: Native,
        rich_text: Native,
        author_suppression: Unsupported,
        escaping: Escaping::Latex,
    },
    SyntaxCapabilities {
        syntax: KeySyntax::Natbib,
        command: None,
        forms: &[Normal, Narrative, CitationForm::AuthorOnly, YearOnly],
        single_item_forms: &[Narrative, CitationForm::AuthorOnly, YearOnly],
        locator: SingleItem,
        prefix: &[
            (Normal, SingleItem),
            (Narrative, Flattened),
            (CitationForm::AuthorOnly, Flattened),
            (YearOnly, SingleItem),
        ],
        suffix: SingleItem,
        rich_text: Native,
        author_suppression: SingleItem,
        escaping: Escaping::Latex,
    },
    SyntaxCapabilities {
        syntax: KeySyntax::Latex,
        command: Some("cite"),
        forms: &[Normal],
        single_item_forms: &[],
        locator: Native,
        prefix: &[(Normal, Flattened)],
        suffix: Native,
        rich_text: Native,
        author_suppression: Unsupported,
        escaping: Escaping::Latex,
    },
    SyntaxCapabilities {
        syntax: KeySyntax::Typst,
        command: None,
        forms: &[
            Normal,
            Narrative,
            CitationForm::AuthorOnly,
            YearOnly,
            CitationForm::Full,
        ],
        single_item_forms: &[],
        locator: Native,
        prefix: &[
            (Normal, Flattened),
            (Narrative, Flattened),
            (CitationForm::AuthorOnly, Flattened),
            (YearOnly, Flattened),
            (CitationForm::Full, Flattened),
        ],
        suffix: Native,
        rich_text: Native,
        author_suppression: Native,
        escaping: Escaping::Typst,
    },
    SyntaxCapabilities {
        syntax: KeySyntax::OrgCite,
        command: None,
        forms: &[Normal, Narrative, CitationForm::AuthorOnly, YearOnly],
        single_item_forms: &[Narrative, CitationForm::AuthorOnly, YearOnly],
        locator: Native,
        prefix: &[
            (Normal, Native),
            (Narrative, Native),
            (CitationForm::AuthorOnly, Native),
            (YearOnly, Native),
        ],
        suffix: Native,
        rich_text: Flattened,
        author_suppression: SingleItem,
        escaping: Escaping::Org,
    },
    SyntaxCapabilities {
        syntax: KeySyntax::Myst,
        command: None,
        forms: &[Normal, Narrative],
        single_item_forms: &[],
        locator: Unsupported,
        prefix: &[(Normal, Unsupported), (Narrative, Unsupported)],
        suffix: Unsupported,
        rich_text: Unsupported,
        author_suppression: Unsupported,
        escaping: Escaping::Myst,
    },
    SyntaxCapabilities {
        syntax: KeySyntax::MultiMarkdown,
        command: None,
        forms: &[Normal],
        single_item_forms: &[],
        locator: Native,
        prefix: &[(Normal, Unsupported)],
        suffix: Native,
        rich_text: Flattened,
        author_suppression: Unsupported,
        escaping: Escaping::MultiMarkdown,
    },
    SyntaxCapabilities {
        syntax: KeySyntax::AsciiDoc,
        command: None,
        forms: &[Normal],
        single_item_forms: &[],
        locator: Native,
        prefix: &[(Normal, Unsupported)],
        suffix: Unsupported,
        rich_text: Unsupported,
        author_suppression: Unsupported,
        escaping: Escaping::AsciiDoc,
    },
    SyntaxCapabilities {
        syntax: KeySyntax::Jupyter,
        command: None,
        forms: &[Normal],
        single_item_forms: &[],
        locator: Unsupported,
        prefix: &[(Normal, Unsupported)],
        suffix: Unsupported,
        rich_text: Unsupported,
        author_suppression: Unsupported,
        escaping: Escaping::Html,
    },
    SyntaxCapabilities {
        syntax: KeySyntax::JekyllScholar,
        command: None,
        forms: &[Normal],
        single_item_forms: &[],
        locator: SingleItem,
        prefix: &[(Normal, Unsupported)],
        suffix: Unsupported,
        rich_text: Unsupported,
        author_suppression: Unsupported,
        escaping: Escaping::Liquid,
    },
    SyntaxCapabilities {
        syntax: KeySyntax::CaywJson,
        command: None,
        forms: &[Normal],
        single_item_forms: &[],
        locator: Native,
        prefix: &[(Normal, Native)],
        suffix: Native,
        rich_text: Flattened,
        author_suppression: Native,
        escaping: Escaping::CaywJson,
    },
    SyntaxCapabilities {
        syntax: KeySyntax::Keys,
        command: None,
        forms: &[Normal],
        single_item_forms: &[],
        locator: Unsupported,
        prefix: &[(Normal, Unsupported)],
        suffix: Unsupported,
        rich_text: Unsupported,
        author_suppression: Unsupported,
        escaping: Escaping::BareKeys,
    },
    SyntaxCapabilities {
        syntax: KeySyntax::CslJson,
        command: None,
        forms: &[Normal],
        single_item_forms: &[],
        locator: Unsupported,
        prefix: &[(Normal, Unsupported)],
        suffix: Unsupported,
        rich_text: Unsupported,
        author_suppression: Unsupported,
        escaping: Escaping::Json,
    },
];

pub fn capabilities(output: &OutputSpec) -> Result<&'static SyntaxCapabilities, CitationError> {
    let OutputSpec::Key { syntax, command } = output else {
        return Err(CitationError::UnsupportedOutput);
    };
    let command = match (syntax, command.as_deref()) {
        (KeySyntax::Biblatex, None) => Some("autocite"),
        (KeySyntax::Latex, None) => Some("cite"),
        (_, command) => command,
    };
    CAPABILITIES
        .iter()
        .find(|entry| entry.syntax == *syntax && entry.command == command)
        .ok_or(CitationError::UnsupportedOutput)
}

struct Pick<'a> {
    index: usize,
    item: &'a CitationItem,
    record: &'a Record,
    key: &'a str,
    form: CitationForm,
    suppress: bool,
}

/// The immutable snapshot is the complete resolution boundary. Engine callers
/// choose their newest snapshot atomically before invoking this pure method.
pub fn cite(snapshot: &Snapshot, request: &CitationRequest) -> Result<CitationResult, CitationError> {
    if request.items.is_empty() {
        return Err(CitationError::EmptySelection);
    }
    if request.items.len() > MAX_CITATION_ITEMS {
        return Err(CitationError::TooManyItems);
    }
    let caps = capabilities(&request.output)?;
    validate_request(request)?;
    let owners = key_owners(snapshot);
    let mut outcomes = Vec::with_capacity(request.items.len());
    let mut picks = Vec::with_capacity(request.items.len());
    let mut losses = Vec::new();
    let mut key_bytes = 0_usize;
    for (index, item) in request.items.iter().enumerate() {
        match resolve(snapshot, item, request.form, caps, request.items.len(), &owners) {
            Err(error) => outcomes.push(ItemOutcome::Refused { item: index, error }),
            Ok((record, key, form)) => {
                // A MyST role applies one form to every key it contains.
                if caps.syntax == KeySyntax::Myst && picks.first().is_some_and(|first: &Pick<'_>| first.form != form) {
                    outcomes.push(ItemOutcome::Refused {
                        item: index,
                        error: CitationError::UnsupportedForm,
                    });
                    continue;
                }
                key_bytes = key_bytes.saturating_add(key.len());
                if key_bytes > MAX_CITATION_BYTES {
                    return Err(CitationError::InvalidKey);
                }
                let supports = |support| support == Native || support == SingleItem && request.items.len() == 1;
                let prefix = if escape::carries(caps.escaping, &item.prefix, false) {
                    caps.prefix_support(form)
                } else {
                    Unsupported
                };
                let suffix = if escape::carries(caps.escaping, &item.suffix, true) {
                    caps.suffix
                } else {
                    Unsupported
                };
                if caps.rich_text == Flattened
                    && [(&item.prefix, prefix), (&item.suffix, suffix)]
                        .into_iter()
                        .any(|(runs, support)| {
                            supports(support)
                                && runs
                                    .iter()
                                    .any(|run| run.italic || run.bold || run.script != Script::Normal)
                        })
                {
                    losses.push(Loss {
                        item: Some(index),
                        field: LossField::RichText,
                        reason: LossReason::Flattened,
                    });
                }
                for (field, present, support) in [
                    (LossField::Locator, item.locator.is_some(), caps.locator),
                    (LossField::Prefix, !item.prefix.is_empty(), prefix),
                    (LossField::Suffix, !item.suffix.is_empty(), suffix),
                    (
                        LossField::AuthorSuppression,
                        item.suppress_author,
                        caps.author_suppression,
                    ),
                ] {
                    if present && !supports(support) {
                        losses.push(Loss {
                            item: Some(index),
                            field,
                            reason: if support == Flattened {
                                LossReason::Flattened
                            } else {
                                LossReason::Unsupported
                            },
                        });
                    }
                }
                if caps.escaping == Escaping::Pandoc
                    && item
                        .prefix
                        .iter()
                        .chain(&item.suffix)
                        .any(|run| run.script != Script::Normal && run.text.chars().any(char::is_whitespace))
                {
                    losses.push(Loss {
                        item: Some(index),
                        field: LossField::RichText,
                        reason: LossReason::Flattened,
                    });
                }
                picks.push(Pick {
                    index,
                    item,
                    record,
                    key,
                    form,
                    suppress: item.suppress_author && supports(caps.author_suppression) || form == YearOnly,
                });
                outcomes.push(ItemOutcome::Rendered { item: index });
            }
        }
    }
    let text = render::render(caps, &picks, request.items.len())?;
    if text.len() > MAX_OUTPUT_BYTES {
        return Err(CitationError::OutputTooLarge);
    }
    let display = if text.is_empty() {
        DisplayTree::default()
    } else {
        DisplayTree {
            nodes: vec![DisplayNode {
                item: (picks.len() == 1).then(|| picks[0].index),
                content: vec![TextRun {
                    text: text.clone(),
                    ..TextRun::default()
                }],
            }],
        }
    };
    Ok(CitationResult::new(text, None, None, display, outcomes, losses, vec![]))
}

fn key_owners(snapshot: &Snapshot) -> HashMap<&str, Option<&ItemRef>> {
    let mut owners = HashMap::new();
    for object in snapshot.objects() {
        if let LibraryObject::Record(record) = &object.object {
            for key in record
                .citation_keys
                .iter()
                .map(|key| key.value.as_str())
                .chain(record.aliases.iter().map(String::as_str))
            {
                owners
                    .entry(key)
                    .and_modify(|owner| {
                        if *owner != Some(&record.id) {
                            *owner = None;
                        }
                    })
                    .or_insert(Some(&record.id));
            }
        }
    }
    owners
}

fn resolve<'a>(
    snapshot: &'a Snapshot,
    item: &CitationItem,
    default_form: CitationForm,
    caps: &SyntaxCapabilities,
    count: usize,
    owners: &HashMap<&str, Option<&ItemRef>>,
) -> Result<(&'a Record, &'a str, CitationForm), CitationError> {
    let object = snapshot.get(&item.item).ok_or(CitationError::MissingItem)?;
    if item
        .expected_cite
        .as_ref()
        .is_some_and(|expected| expected != &object.fingerprints.cite)
    {
        return Err(CitationError::ChangedItem);
    }
    let LibraryObject::Record(record) = &object.object else {
        return Err(CitationError::NotCitable);
    };
    validate_annotation(snapshot, item)?;
    let form = item.form.unwrap_or(default_form);
    if !caps.forms.contains(&form)
        || count != 1 && caps.single_item_forms.contains(&form)
        || item.suppress_author && matches!(form, Narrative | CitationForm::AuthorOnly | CitationForm::Full)
        // Natbib's numerical mode silently discards author-only postnotes.
        // The key renderer cannot know the destination document's mode.
        || caps.syntax == KeySyntax::Natbib
            && form == CitationForm::AuthorOnly
            && (item.locator.is_some() || item.suffix.iter().any(|run| !run.text.trim().is_empty()))
    {
        return Err(CitationError::UnsupportedForm);
    }
    let key = record
        .citation_key()
        .map_err(|error| match error {
            ModelError::ConflictingCitationKeys => CitationError::ConflictingKey,
            _ => CitationError::InvalidKey,
        })?
        .ok_or(CitationError::MissingKey)?;
    if owners.get(key) != Some(&Some(&record.id)) {
        return Err(CitationError::AmbiguousKey);
    }
    if key.is_empty()
        || key.len() > 64 * 1024
        || key
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, ',' | '{' | '}'))
        || caps.escaping == Escaping::Latex && key.contains(['%', '#', '\\', '~', '^', '$', '&'])
        || !escape::key_fits(caps.escaping, key)
    {
        return Err(CitationError::InvalidKey);
    }
    if item.locator.as_ref().is_some_and(|locator| {
        locator.value.trim().is_empty()
            || locator.value.chars().any(char::is_control)
            || !escape::locator_fits(caps.escaping, locator)
    }) {
        return Err(CitationError::InvalidLocator);
    }
    if caps.escaping == Escaping::Pandoc
        && item.suffix.iter().any(|run| !run.text.trim().is_empty())
        && let Some(locator) = &item.locator
        && locator.value.contains(['{', '}', '[', ']', '\\'])
    {
        // Pandoc reparses the Markdown inlines as a braced locator. Nested
        // delimiters or escapes cannot preserve its boundary with a suffix.
        return Err(CitationError::InvalidLocator);
    }
    if item
        .prefix
        .iter()
        .chain(&item.suffix)
        .any(|run| run.text.chars().any(char::is_control))
    {
        return Err(CitationError::InvalidRichText);
    }
    Ok((record, key, form))
}

fn validate_annotation(snapshot: &Snapshot, item: &CitationItem) -> Result<(), CitationError> {
    let Some(id) = &item.annotation else {
        return if item.expected_annotation_cite.is_some() {
            Err(CitationError::AnnotationMismatch)
        } else {
            Ok(())
        };
    };
    let object = snapshot.get(id).ok_or(CitationError::MissingItem)?;
    if item
        .expected_annotation_cite
        .as_ref()
        .is_some_and(|expected| expected != &object.fingerprints.cite)
    {
        return Err(CitationError::ChangedItem);
    }
    let LibraryObject::Annotation(annotation) = &object.object else {
        return Err(CitationError::AnnotationMismatch);
    };
    let Some(parent) = snapshot.get(&annotation.parent) else {
        return Err(CitationError::AnnotationMismatch);
    };
    let LibraryObject::Attachment(attachment) = &parent.object else {
        return Err(CitationError::AnnotationMismatch);
    };
    if attachment.parent.as_ref() != Some(&item.item) {
        return Err(CitationError::AnnotationMismatch);
    }
    match (&annotation.page_label, &item.locator) {
        (Some(page), Some(locator)) if locator.term == LocatorTerm::Page && page.trim() == locator.value.trim() => {
            Ok(())
        }
        _ => Err(CitationError::AnnotationMismatch),
    }
}

fn validate_request(request: &CitationRequest) -> Result<(), CitationError> {
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
