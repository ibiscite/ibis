//! Zotero library identity. Talking to Zotero is a reader's job
//! (`ibis_sources::zotero_local`).

use std::fmt;

/// Which Zotero library items belong to. Identity is the library plus the
/// item key, never a citation key (ADR 0006).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
#[schemars(rename = "ZoteroLibrary")]
pub enum Library {
    /// The personal library ("My Library").
    User,
    /// A group library, by its numeric group ID.
    Group(#[schemars(range(min = 1, max = 9_223_372_036_854_775_807_u64))] u64),
}

impl fmt::Display for Library {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Library::User => f.write_str("zotero:user"),
            Library::Group(id) => write!(f, "zotero:group:{id}"),
        }
    }
}
