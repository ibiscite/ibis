//! Method parameters. File locations are headless/native input only and never
//! appear in method results or diagnostics; the app exposes a narrower surface.
use crate::Method;
use ibis_core::contract::{CitationForm, CitationRequest, ItemRef, LibraryId, OutputSpec};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "kebab-case", deny_unknown_fields)]
pub enum LibrariesRequest {
    #[default]
    List,
    OpenFile {
        path: PathBuf,
    },
    OpenZotero {
        #[serde(default)]
        data_directory: Option<PathBuf>,
    },
    Reload {
        #[serde(default)]
        library: Option<LibraryId>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchRequest {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Omitted searches every loaded library; an empty list searches none.
    #[serde(default)]
    pub libraries: Option<Vec<LibraryId>>,
}
fn default_limit() -> usize {
    10
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "by", content = "value", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ItemSelector {
    Id(ItemRef),
    Key(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShowRequest {
    pub item: ItemSelector,
}

/// Human key arguments are resolved against one engine snapshot, never by a frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyCitationRequest {
    pub keys: Vec<String>,
    pub output: OutputSpec,
    #[serde(default)]
    pub form: CitationForm,
    #[serde(default)]
    pub output_locale: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StylesRequest {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SchemaRequest {
    #[serde(default)]
    pub method: Option<Method>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CallRequest {
    pub method: Method,
    #[serde(default)]
    pub params: Value,
}

/// Generated from the same types the dispatcher deserializes.
pub fn request_schema(method: Method) -> Value {
    match method {
        Method::Libraries => schema_for!(LibrariesRequest),
        Method::Search => schema_for!(SearchRequest),
        Method::Show => schema_for!(ShowRequest),
        Method::Cite => schema_for!(CitationRequest),
        Method::Styles => schema_for!(StylesRequest),
        Method::Schema => schema_for!(SchemaRequest),
        Method::Call => schema_for!(CallRequest),
    }
    .into()
}
