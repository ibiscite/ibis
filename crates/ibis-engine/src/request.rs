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
    /// Read Zotero. `libraries` replaces the chosen libraries (at first the
    /// personal library only); omitted, the current choice is read again.
    OpenZotero {
        #[serde(default)]
        #[schemars(length(min = 1, max = 32))]
        libraries: Option<Vec<ZoteroLibrary>>,
    },
    OpenFile {
        path: PathBuf,
    },
    Reload {
        #[serde(default)]
        library: Option<LibraryId>,
    },
}
/// A Zotero library: the personal library ("My Library") or a group by its
/// Zotero group ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum ZoteroLibrary {
    Personal,
    Group(#[schemars(range(min = 1, max = 9_223_372_036_854_775_807_u64))] u64),
}
impl From<ZoteroLibrary> for ibis_sources::zotero_source::Library {
    fn from(library: ZoteroLibrary) -> Self {
        match library {
            ZoteroLibrary::Personal => Self::User,
            ZoteroLibrary::Group(id) => Self::Group(id),
        }
    }
}
impl From<ibis_sources::zotero_source::Library> for ZoteroLibrary {
    fn from(library: ibis_sources::zotero_source::Library) -> Self {
        match library {
            ibis_sources::zotero_source::Library::User => Self::Personal,
            ibis_sources::zotero_source::Library::Group(id) => Self::Group(id),
        }
    }
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
    /// Read a locator typed at the end of the query (`smith p 45`) and search
    /// for the rest; the result says what was read so it can be undone.
    #[serde(default)]
    pub interpret_locator: bool,
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
