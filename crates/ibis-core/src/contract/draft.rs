//! Shared client drafts and citation presentation.
use super::*;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DraftItem {
    pub id: ItemRef,
    #[schemars(with = "String")]
    pub cite: Fingerprint,
    /// A canonical locator captured from search; its value is never parsed again.
    pub locator: Option<super::Locator>,
    /// Raw locator field text and its chosen type, mutually exclusive with a
    /// captured locator.
    pub locator_input: Option<LocatorInput>,
    pub form: Option<CitationForm>,
    /// Literal text; only the engine escapes and formats it.
    pub prefix: Option<String>,
    pub suffix: Option<String>,
}
/// A formatted bibliography, shown before it is copied. The picker's own
/// citation receipt is untouched.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SessionBibliography {
    pub text: String,
    pub display: DisplayTree,
    pub content_hash: String,
    pub warnings: Vec<String>,
    /// References this style cannot write, by key or title. They are named so
    /// that the rest can still be used.
    pub left_out: Vec<String>,
}
/// What a person typed in a reference's locator field, with the type its pop-up
/// shows. Only the engine reads it: a typed label (`ch. 3`) names the type.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocatorInput {
    pub label: LocatorTerm,
    pub text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FormOption {
    pub form: CitationForm,
    pub example: Option<String>,
    pub reason: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FormDetails {
    pub forms: Vec<FormOption>,
    pub current_form: Option<CitationForm>,
}
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Preview {
    /// The effective output after an editor request overrides the picker.
    pub effective_output: Option<String>,
    pub ticket: String,
    pub content_hash: String,
    pub text: String,
    pub warnings: Vec<String>,
    pub display: DisplayTree,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<crate::contract::Problem>,
    /// Why this key citation is refused: a key in the project bibliography
    /// names a different work.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_refusal: Option<String>,
    /// What the engine says about this citation that is no warning, such as a
    /// full first note; the web view words each from the catalogue.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<Notice>,
}

/// A citation submitted by a client. Raw editor fields are resolved only by the engine.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub items: Vec<DraftItem>,
    pub output: OutputSpec,
    #[serde(default)]
    pub form: CitationForm,
    #[serde(default)]
    pub keep_order: bool,
    #[serde(default)]
    pub input_language: Option<String>,
    #[serde(default)]
    pub output_locale: Option<String>,
    #[serde(default)]
    pub sample: bool,
}

/// A transport operation's identity, including an abort that can arrive first.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CitationIntent {
    #[serde(default)]
    pub keep_order: bool,
    pub page: u32,
    pub request: u32,
    pub cancel: bool,
    #[serde(default)]
    pub context: Option<(u32, u32)>,
}
