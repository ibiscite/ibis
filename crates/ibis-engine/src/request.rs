//! Method parameters. File locations are headless/native input only and never
//! appear in method results or diagnostics; the app exposes a narrower surface.
use crate::Method;
use ibis_core::contract::{CitationForm, CitationRequest, ItemRef, LibraryId, Locator, OutputSpec, RichText};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

/// Every action is a struct, even with no fields: serde refuses unknown fields
/// only in those, and an argument nothing reads must be refused, not ignored.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "kebab-case", deny_unknown_fields)]
pub enum LibrariesRequest {
    List {},
    /// Read Zotero. `libraries` replaces the chosen libraries (at first the
    /// personal library only); omitted, the current choice is read again.
    OpenZotero {
        #[serde(default)]
        #[schemars(length(min = 1, max = 32))]
        libraries: Option<Vec<ZoteroLibrary>>,
        /// The Zotero profile to read while Zotero is closed, by the name
        /// `zotero-profiles` gives; never a location. Omitted, a computer with
        /// one profile reads it and one with several is refused.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        profile: Option<String>,
        /// Read everything again, not only what changed.
        #[serde(default)]
        reload: bool,
    },
    /// The Zotero profiles on this computer, by name; reads no library.
    ZoteroProfiles {},
    /// The Zotero group libraries known now, by name: asked of Zotero while
    /// it runs, otherwise those its closed copy listed when last read.
    ZoteroGroups {},
    OpenFile {
        path: PathBuf,
    },
    Reload {
        #[serde(default)]
        library: Option<LibraryId>,
    },
}
impl Default for LibrariesRequest {
    fn default() -> Self {
        Self::List {}
    }
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
    pub keys: Vec<KeyItem>,
    pub output: OutputSpec,
    #[serde(default)]
    pub form: CitationForm,
    #[serde(default)]
    pub output_locale: Option<String>,
}

/// Exact keys to look for in one project `.bib`, which stays out of the search
/// libraries. Reading it never writes to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectRequest {
    pub path: PathBuf,
    pub keys: Vec<String>,
}

/// One reference's original BibTeX entry, with what it needs to stand alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EntryRequest {
    pub item: ItemSelector,
}

/// A page of the style list: the curated styles first, then the rest by id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StylesRequest {
    /// Words from a style's title or id; every word must match.
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default = "default_styles_limit")]
    #[schemars(range(min = 1, max = 20_000))]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}
impl Default for StylesRequest {
    fn default() -> Self {
        Self {
            query: None,
            limit: default_styles_limit(),
            offset: 0,
        }
    }
}
fn default_styles_limit() -> usize {
    100
}

/// The `outputs` method takes no parameters.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputsRequest {}

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
        Method::Project => schema_for!(ProjectRequest),
        Method::Entry => schema_for!(EntryRequest),
        Method::Styles => schema_for!(StylesRequest),
        Method::Outputs => schema_for!(OutputsRequest),
        Method::Schema => schema_for!(SchemaRequest),
        Method::Call => schema_for!(CallRequest),
    }
    .into()
}

/// One citation key with the options typed after it, as the picker would add them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyItem {
    pub key: String,
    #[serde(default)]
    pub locator: Option<Locator>,
    #[serde(default)]
    pub prefix: RichText,
    #[serde(default)]
    pub suffix: RichText,
    #[serde(default)]
    pub suppress_author: bool,
}
impl From<String> for KeyItem {
    fn from(key: String) -> Self {
        Self { key, ..Self::default() }
    }
}
impl From<&str> for KeyItem {
    fn from(key: &str) -> Self {
        key.to_owned().into()
    }
}
