//! Pure DOI lookup over the complete open-library snapshot.
//!
//! Permission is an observation from settings::Owner, never stored here.
//! An outbound intent describes approved data; it performs no network I/O.

use ibis_core::{
    contract::{LibraryObject, Snapshot, SnapshotObject},
    identifier::{self, Identifier},
};
use serde::Serialize;

#[derive(Debug)]
pub enum Resolution<'a> {
    /// All exact matches, retaining their existing library/citation ownership.
    Local(Vec<&'a SnapshotObject>),
    NeedsApproval(Disclosure),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    UnsupportedQuery,
    Disabled,
}

/// Resolve against the complete snapshot, without search ranking or a hit cap.
/// Local matches win even when permission is absent or off. Only `Some(true)`
/// permits a local miss to offer disclosure; it never authorizes an intent.
pub fn resolve<'a>(query: &str, libraries: &'a Snapshot, permission: Option<bool>) -> Result<Resolution<'a>, Refusal> {
    let Some(Identifier::Doi(doi)) = identifier::parse_query(query) else {
        return Err(Refusal::UnsupportedQuery);
    };
    let matches: Vec<_> = libraries
        .objects()
        .iter()
        .filter_map(|item| match &item.object {
            LibraryObject::Record(record)
                if identifier::of_record(record)
                    .iter()
                    .any(|id| matches!(id, Identifier::Doi(value) if value == &doi)) =>
            {
                Some(item.as_ref())
            }
            _ => None,
        })
        .collect();
    if !matches.is_empty() {
        return Ok(Resolution::Local(matches));
    }
    if permission != Some(true) {
        return Err(Refusal::Disabled);
    }
    Ok(Resolution::NeedsApproval(Disclosure { doi }))
}

/// Show these exact canonical identifier bytes and services before deciding.
/// No approval survives declining or dropping this disclosure.
#[derive(Debug)]
pub struct Disclosure {
    doi: String,
}

impl Disclosure {
    pub fn doi(&self) -> &str {
        &self.doi
    }

    pub fn services(&self) -> [&'static str; 3] {
        ["doi.org", "api.crossref.org", "data.crosscite.org"]
    }

    /// The adapter binds the person's decision to this disclosure instance.
    pub fn decide(self, approved: bool) -> Option<Approval> {
        approved.then_some(Approval { doi: self.doi })
    }
}

/// An in-memory, single-use approval of the disclosed DOI. No public
/// constructor, cloning or deserialization can manufacture another approval.
#[derive(Debug)]
pub struct Approval {
    doi: String,
}

impl Approval {
    /// Observe permission again immediately before consumption. Refusal also
    /// consumes the approval. There is no replacement identifier argument.
    pub fn prepare(self, current_permission: Option<bool>) -> Result<OutboundIntent, Refusal> {
        if current_permission != Some(true) {
            return Err(Refusal::Disabled);
        }
        Ok(OutboundIntent { doi: self.doi })
    }
}

/// The entire outbound payload: only the approved DOI, with no record data.
/// This is intent, not evidence that a request was made or safely transported.
#[derive(Debug, Serialize)]
pub struct OutboundIntent {
    doi: String,
}

impl OutboundIntent {
    pub fn doi(&self) -> &str {
        &self.doi
    }
}
