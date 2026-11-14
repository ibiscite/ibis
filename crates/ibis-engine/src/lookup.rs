//! Pure DOI lookup over the complete open-library snapshot.
//!
//! Resolving a DOI only reads local records; a miss returns its canonical DOI.

use ibis_core::{
    contract::{LibraryObject, Snapshot, SnapshotObject},
    identifier::{self, Identifier},
};

#[derive(Debug)]
pub enum Resolution<'a> {
    /// All exact matches, retaining their existing library/citation ownership.
    Local(Vec<&'a SnapshotObject>),
    /// No local record holds this DOI. This does not authorize a network lookup.
    Missing { doi: String },
}

#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    UnsupportedQuery,
}

/// Resolve against the complete snapshot, without search ranking or a hit cap.
pub fn resolve<'a>(query: &str, libraries: &'a Snapshot) -> Result<Resolution<'a>, Refusal> {
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
    Ok(Resolution::Missing { doi })
}
