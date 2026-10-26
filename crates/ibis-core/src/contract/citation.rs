//! Shared citation intent and output. Rendering belongs to the syntax/CSL engines.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::model::{Fingerprint, ItemRef};

/// CSL 1.0.2 input locator vocabulary, including the disambiguating `-locator` names.
/// https://github.com/citation-style-language/schema/blob/v1.0.2/schemas/input/csl-citation.json
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum LocatorTerm {
    Act,
    Appendix,
    ArticleLocator,
    Book,
    Canon,
    Chapter,
    Column,
    Elocation,
    Equation,
    Figure,
    Folio,
    Issue,
    Line,
    Note,
    Opus,
    Page,
    Paragraph,
    Part,
    Rule,
    Scene,
    Section,
    SubVerbo,
    Supplement,
    Table,
    Timestamp,
    TitleLocator,
    Verse,
    Version,
    Volume,
}

impl LocatorTerm {
    pub const ALL: [Self; 29] = [
        Self::Act,
        Self::Appendix,
        Self::ArticleLocator,
        Self::Book,
        Self::Canon,
        Self::Chapter,
        Self::Column,
        Self::Elocation,
        Self::Equation,
        Self::Figure,
        Self::Folio,
        Self::Issue,
        Self::Line,
        Self::Note,
        Self::Opus,
        Self::Page,
        Self::Paragraph,
        Self::Part,
        Self::Rule,
        Self::Scene,
        Self::Section,
        Self::SubVerbo,
        Self::Supplement,
        Self::Table,
        Self::Timestamp,
        Self::TitleLocator,
        Self::Verse,
        Self::Version,
        Self::Volume,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Locator {
    pub term: LocatorTerm,
    /// Literal range or pinpointer, never interpreted as markup.
    pub value: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Script {
    #[default]
    Normal,
    Subscript,
    Superscript,
}

/// Fixed-depth rich text: styling is data, never arbitrary HTML or executable markup.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextRun {
    pub text: String,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub script: Script,
}

pub type RichText = Vec<TextRun>;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CitationForm {
    #[default]
    Normal,
    Narrative,
    AuthorOnly,
    YearOnly,
    Full,
    BibliographyOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CitationItem {
    pub item: ItemRef,
    /// A picked item carries its original cite fingerprint for atomic copy-time validation.
    /// Direct requests may omit it when they intentionally use the current snapshot.
    #[serde(default)]
    pub expected_cite: Option<Fingerprint>,
    /// Overrides the request form, allowing mixed forms in one citation.
    #[serde(default)]
    pub form: Option<CitationForm>,
    #[serde(default)]
    pub locator: Option<Locator>,
    #[serde(default)]
    pub prefix: RichText,
    #[serde(default)]
    pub suffix: RichText,
    #[serde(default)]
    pub suppress_author: bool,
    #[serde(default)]
    pub annotation: Option<ItemRef>,
    /// Captures the annotation page/parent intent separately from its cited work.
    #[serde(default)]
    pub expected_annotation_cite: Option<Fingerprint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum KeySyntax {
    Pandoc,
    Biblatex,
    Natbib,
    Latex,
    Typst,
    OrgCite,
    Myst,
    MultiMarkdown,
    AsciiDoc,
    Jupyter,
    JekyllScholar,
    ScannableCite,
    Keys,
    CslJson,
    CaywJson,
}

impl KeySyntax {
    pub const ALL: [Self; 15] = [
        Self::Pandoc,
        Self::Biblatex,
        Self::Natbib,
        Self::Latex,
        Self::Typst,
        Self::OrgCite,
        Self::Myst,
        Self::MultiMarkdown,
        Self::AsciiDoc,
        Self::Jupyter,
        Self::JekyllScholar,
        Self::ScannableCite,
        Self::Keys,
        Self::CslJson,
        Self::CaywJson,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum OutputSpec {
    Key {
        syntax: KeySyntax,
        /// A command name for syntaxes that support it; engines validate its grammar.
        #[serde(default)]
        command: Option<String>,
    },
    Csl {
        style: String,
    },
    Preset {
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CitationRequest {
    /// Input order and duplicate references are meaningful and must be preserved.
    pub items: Vec<CitationItem>,
    #[serde(default)]
    pub form: CitationForm,
    #[serde(default)]
    pub keep_order: bool,
    pub output: OutputSpec,
    #[serde(default)]
    /// None uses en-US, never the process or operating-system locale.
    pub input_language: Option<String>,
    #[serde(default)]
    /// None uses the style default. Engines validate locale strings as data.
    pub output_locale: Option<String>,
}

/// Fixed citation diagnostics contain no source data, paths, keys or request text.
/// A front-end boundary maps a wholly invalid/cancelled request to its envelope error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CitationError {
    EmptySelection,
    TooManyItems,
    InvalidLocator,
    InvalidRichText,
    UnsupportedForm,
    MissingItem,
    ChangedItem,
    MissingKey,
    AmbiguousKey,
    ConflictingKey,
    InvalidKey,
    UnsupportedOutput,
    InvalidLocale,
    Cancelled,
    FormatterUnavailable,
    AnnotationMismatch,
    NotCitable,
}

impl CitationError {
    pub const ALL: [Self; 17] = [
        Self::EmptySelection,
        Self::TooManyItems,
        Self::InvalidLocator,
        Self::InvalidRichText,
        Self::UnsupportedForm,
        Self::MissingItem,
        Self::ChangedItem,
        Self::MissingKey,
        Self::AmbiguousKey,
        Self::ConflictingKey,
        Self::InvalidKey,
        Self::UnsupportedOutput,
        Self::InvalidLocale,
        Self::Cancelled,
        Self::FormatterUnavailable,
        Self::AnnotationMismatch,
        Self::NotCitable,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "outcome", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ItemOutcome {
    Rendered { item: usize },
    Refused { item: usize, error: CitationError },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum LossField {
    Locator,
    Prefix,
    Suffix,
    AuthorSuppression,
    RichText,
    Order,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum LossReason {
    Unsupported,
    Flattened,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Loss {
    /// Zero-based request index; None describes a request-wide loss.
    pub item: Option<usize>,
    pub field: LossField,
    pub reason: LossReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Notice {
    NeedsDocumentContext,
    ReferenceEntry,
    FullFirstNote,
    YearSuffixWithheld,
    InputLanguageUnsupported,
}

impl Notice {
    pub const ALL: [Self; 5] = [
        Self::NeedsDocumentContext,
        Self::ReferenceEntry,
        Self::FullFirstNote,
        Self::YearSuffixWithheld,
        Self::InputLanguageUnsupported,
    ];
}

/// One display group owns ordered text leaves; shared punctuation has no item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DisplayNode {
    pub item: Option<usize>,
    pub content: RichText,
}

/// A bounded-depth tree: root -> ownership groups -> styled text leaves.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DisplayTree {
    pub nodes: Vec<DisplayNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CitationResult {
    pub text: String,
    pub html: Option<String>,
    pub rtf: Option<String>,
    pub display: DisplayTree,
    /// Exactly one outcome per request item, addressed by its zero-based index.
    pub outcomes: Vec<ItemOutcome>,
    pub losses: Vec<Loss>,
    pub notices: Vec<Notice>,
    /// Digest of the exact clipboard representations, including their presence.
    pub content_hash: Fingerprint,
}

/// Borrowed canonical representations, available only after the copy gate succeeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipboardContent<'a> {
    pub text: &'a str,
    pub html: Option<&'a str>,
    pub rtf: Option<&'a str>,
}

impl CitationResult {
    /// Outcomes are complete only when every item is represented once, in input order,
    /// without any refusal. The request boundary additionally checks the expected count.
    pub fn is_complete(&self) -> bool {
        !self.outcomes.is_empty()
            && self
                .outcomes
                .iter()
                .enumerate()
                .all(|(index, outcome)| matches!(outcome, ItemOutcome::Rendered { item } if *item == index))
    }

    /// Copy must use the original request's item count, so omitted trailing outcomes
    /// cannot turn a partially rendered request into an apparently successful citation.
    pub fn copy_content(&self, expected_items: usize) -> Option<ClipboardContent<'_>> {
        (self.outcomes.len() == expected_items
            && self.is_complete()
            && !self.text.trim().is_empty()
            && self.content_matches_hash())
        .then_some(ClipboardContent {
            text: &self.text,
            html: self.html.as_deref(),
            rtf: self.rtf.as_deref(),
        })
    }

    pub fn new(
        text: String,
        html: Option<String>,
        rtf: Option<String>,
        display: DisplayTree,
        outcomes: Vec<ItemOutcome>,
        losses: Vec<Loss>,
        notices: Vec<Notice>,
    ) -> Self {
        let content_hash = content_hash(&text, html.as_deref(), rtf.as_deref());
        Self {
            text,
            html,
            rtf,
            display,
            outcomes,
            losses,
            notices,
            content_hash,
        }
    }

    pub fn content_matches_hash(&self) -> bool {
        self.content_hash == content_hash(&self.text, self.html.as_deref(), self.rtf.as_deref())
    }
}

fn content_hash(text: &str, html: Option<&str>, rtf: Option<&str>) -> Fingerprint {
    let mut bytes = b"ibis-citation-content-v1".to_vec();
    for value in [Some(text), html, rtf] {
        match value {
            None => bytes.push(0),
            Some(value) => {
                bytes.push(1);
                bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
                bytes.extend_from_slice(value.as_bytes());
            }
        }
    }
    Fingerprint::of_bytes(&bytes)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::super::model::LibraryId;
    use super::*;
    use serde::{Serialize, de::DeserializeOwned};
    use serde_json::json;
    use std::fmt::Debug;

    fn roundtrip<T: Serialize + DeserializeOwned + PartialEq + Debug>(value: T) {
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(serde_json::from_str::<T>(&json).unwrap(), value);
    }

    fn item() -> CitationItem {
        CitationItem {
            item: ItemRef {
                library: LibraryId::file(b"synthetic-library"),
                key: "work-1".into(),
            },
            expected_cite: None,
            form: None,
            locator: None,
            prefix: vec![],
            suffix: vec![],
            suppress_author: false,
            annotation: None,
            expected_annotation_cite: None,
        }
    }

    #[test]
    fn all_29_csl_locator_terms_roundtrip() {
        let expected = "act appendix article-locator book canon chapter column elocation equation figure folio issue line note opus page paragraph part rule scene section sub-verbo supplement table timestamp title-locator verse version volume";
        assert_eq!(LocatorTerm::ALL.len(), 29);
        for (term, expected) in LocatorTerm::ALL.into_iter().zip(expected.split_whitespace()) {
            assert_eq!(serde_json::to_value(term).unwrap(), expected);
            roundtrip(Locator {
                term,
                value: "12–14, 20".into(),
            });
        }
        assert!(serde_json::from_value::<LocatorTerm>(json!("article")).is_err());
        assert!(serde_json::from_value::<LocatorTerm>(json!("title")).is_err());
    }

    #[test]
    fn request_preserves_duplicates_order_and_mixed_intent() {
        let mut first = item();
        first.locator = Some(Locator {
            term: LocatorTerm::Page,
            value: "١٢–١٤".into(),
        });
        first.prefix = vec![TextRun {
            text: "参照".into(),
            italic: true,
            bold: true,
            script: Script::Superscript,
        }];
        first.expected_cite = Some(Fingerprint::of_bytes(b"citation"));
        let mut second = item();
        second.form = Some(CitationForm::Narrative);
        second.suffix = vec![TextRun {
            text: "é & <脚注>".into(),
            script: Script::Subscript,
            ..TextRun::default()
        }];
        second.suppress_author = true;
        second.expected_annotation_cite = Some(Fingerprint::of_bytes(b"annotation cite intent"));
        second.annotation = Some(ItemRef {
            library: first.item.library.clone(),
            key: "annotation-1".into(),
        });
        roundtrip(CitationRequest {
            items: vec![first, second],
            form: CitationForm::Normal,
            keep_order: true,
            output: OutputSpec::Csl { style: "apa".into() },
            input_language: Some("ar".into()),
            output_locale: Some("ja-JP".into()),
        });
    }

    #[test]
    fn every_form_output_and_result_variant_roundtrips() {
        for form in [
            CitationForm::Normal,
            CitationForm::Narrative,
            CitationForm::AuthorOnly,
            CitationForm::YearOnly,
            CitationForm::Full,
            CitationForm::BibliographyOnly,
        ] {
            roundtrip(form);
        }
        for syntax in KeySyntax::ALL {
            roundtrip(OutputSpec::Key {
                syntax,
                command: Some("autocite".into()),
            });
        }
        roundtrip(OutputSpec::Preset { name: "writing".into() });
        roundtrip(ItemOutcome::Rendered { item: 0 });
        for error in CitationError::ALL {
            roundtrip(ItemOutcome::Refused { item: 0, error });
        }
        for field in [
            LossField::Locator,
            LossField::Prefix,
            LossField::Suffix,
            LossField::AuthorSuppression,
            LossField::RichText,
            LossField::Order,
        ] {
            for reason in [LossReason::Unsupported, LossReason::Flattened] {
                roundtrip(Loss {
                    item: Some(0),
                    field,
                    reason,
                });
            }
        }
        for notice in Notice::ALL {
            roundtrip(notice);
        }
        let mut result = CitationResult::new(
            "合成 & <work>".into(),
            Some("<i>合成 &amp; &lt;work&gt;</i>".into()),
            Some("{\\rtf1 synthetic}".into()),
            DisplayTree {
                nodes: vec![DisplayNode {
                    item: Some(0),
                    content: vec![TextRun {
                        text: "合成 & <work>".into(),
                        italic: true,
                        ..TextRun::default()
                    }],
                }],
            },
            vec![ItemOutcome::Rendered { item: 0 }],
            vec![],
            vec![Notice::NeedsDocumentContext],
        );
        assert!(result.content_matches_hash());
        roundtrip(result.clone());
        result.html = Some("changed".into());
        assert!(!result.content_matches_hash());
    }

    #[test]
    fn copy_requires_every_requested_item_and_untampered_content() {
        let mut result = CitationResult::new(
            "[@one; @two]".into(),
            None,
            None,
            DisplayTree::default(),
            vec![ItemOutcome::Rendered { item: 0 }, ItemOutcome::Rendered { item: 1 }],
            vec![],
            vec![],
        );
        assert!(result.is_complete());
        assert_eq!(result.copy_content(2).unwrap().text, "[@one; @two]");
        assert!(result.copy_content(3).is_none());
        result.outcomes[1] = ItemOutcome::Refused {
            item: 1,
            error: CitationError::MissingKey,
        };
        assert!(result.copy_content(2).is_none());
        for malformed in [
            vec![],
            vec![ItemOutcome::Rendered { item: 0 }, ItemOutcome::Rendered { item: 0 }],
            vec![ItemOutcome::Rendered { item: 0 }, ItemOutcome::Rendered { item: 2 }],
        ] {
            result.outcomes = malformed;
            assert!(!result.is_complete());
            assert!(result.copy_content(result.outcomes.len()).is_none());
        }
        result.outcomes = vec![ItemOutcome::Rendered { item: 0 }];
        result.text.push_str(" tampered");
        assert!(result.copy_content(1).is_none());
    }

    #[test]
    fn copy_requires_visible_canonical_plain_text_even_with_rich_representations() {
        for text in ["", " \t\r\n", "\u{2003}"] {
            let result = CitationResult::new(
                text.into(),
                Some("<i>synthetic</i>".into()),
                Some("{\\rtf1 synthetic}".into()),
                DisplayTree::default(),
                vec![ItemOutcome::Rendered { item: 0 }],
                vec![],
                vec![],
            );
            assert!(result.is_complete());
            assert!(result.content_matches_hash());
            assert!(result.copy_content(1).is_none());
        }
    }

    #[test]
    fn content_hash_distinguishes_representation_boundaries_and_empty_values() {
        assert_ne!(content_hash("ab", Some("c"), None), content_hash("a", Some("bc"), None));
        assert_ne!(content_hash("", None, None), content_hash("", Some(""), None));
        assert_ne!(content_hash("", Some(""), None), content_hash("", None, Some("")));
        assert_ne!(
            content_hash("same", None, Some("one")),
            content_hash("same", None, Some("two"))
        );
    }

    #[test]
    fn hostile_and_large_rich_text_is_plain_data_and_unknown_fields_fail() {
        let text = "<script>\\input{bad} & 文字 \u{202e}".repeat(10_000);
        roundtrip(TextRun {
            text,
            italic: true,
            bold: true,
            script: Script::Normal,
        });
        roundtrip(RichText::new());
        let mut request = json!({"items":[],"output":{"kind":"key","syntax":"pandoc"}});
        let parsed = serde_json::from_value::<CitationRequest>(request.clone()).unwrap();
        assert!(parsed.items.is_empty());
        request["unexpected"] = json!("never silently ignored");
        assert!(serde_json::from_value::<CitationRequest>(request).is_err());
        assert!(serde_json::from_value::<TextRun>(json!({"text":"x","children":[]})).is_err());
        assert!(serde_json::from_value::<TextRun>(json!({"text":"x","script":"both"})).is_err());
        assert!(serde_json::from_value::<OutputSpec>(json!({"kind":"key","syntax":"invented"})).is_err());
    }
}
