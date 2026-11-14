//! Exact local outcomes retained by [`crate::search::SnapshotIndex::pasted_list`].
//! These describe only the supplied snapshot and scope. Source health and
//! completeness belong to the engine; an unmatched entry makes no claim about
//! an unavailable library. No lookup, attachment probing or download occurs.

use crate::{
    contract::{Attachment, SnapshotObject},
    search::MatchField,
};

/// One parsed occurrence, in input order, including repeated occurrences.
#[derive(Debug)]
pub struct EntryMatch<'a> {
    /// The parser's entry label: a decoded key or the supplied identifier spelling.
    pub text: String,
    /// All exact candidates in snapshot order: zero is unmatched in this snapshot,
    /// one is resolved, and more than one is ambiguous. No candidate is chosen.
    pub candidates: Vec<Candidate<'a>>,
}

/// An exact record candidate; citation keys retain the resolver's precedence.
#[derive(Debug)]
pub struct Candidate<'a> {
    /// Always a record from the supplied snapshot.
    pub item: &'a SnapshotObject,
    pub matched: MatchField,
    /// Attached PDFs and HTML/XHTML pages, linked by their full parent identity.
    /// Existing identities, media types and availability are retained verbatim;
    /// an attachment's presence does not imply that it is downloaded or readable.
    /// `Attachment` contains neither paths nor file bytes.
    pub attachments: Vec<&'a Attachment>,
}
