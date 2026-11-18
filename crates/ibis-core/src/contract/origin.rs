//! Where a record came from (decision 0051): a library the person opened, or
//! an online lookup. A looked-up record is in no library until the person adds it.

use super::ItemRef;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The services a DOI lookup may answer from, after `doi.org` hands the DOI on
/// (decision 0008). No other host is ever accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum LookupService {
    #[serde(rename = "api.crossref.org")]
    Crossref,
    #[serde(rename = "data.crosscite.org")]
    Datacite,
}

impl LookupService {
    pub const ALL: [Self; 2] = [Self::Crossref, Self::Datacite];
    pub const fn host(self) -> &'static str {
        match self {
            Self::Crossref => "api.crossref.org",
            Self::Datacite => "data.crosscite.org",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "from", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Origin {
    /// A reference in one of the person's open libraries; nothing was sent.
    Library { item: ItemRef },
    /// A record a service gave for the DOI; it is in none of the person's libraries.
    Lookup { doi: String, service: LookupService },
}
