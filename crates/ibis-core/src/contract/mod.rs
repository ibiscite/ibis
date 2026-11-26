//! Shared engine contract (ADR 0028). Front ends consume these same types.

pub mod adapter;
pub mod citation;
pub mod document;
pub use document::{Cite, Cited, MAX_TEXT_BYTES, Manuscript, Place, ScanFinding, ScanProblem, TextKind};
mod csl;
pub use csl::{CompactName, Csl, CslRef};
pub mod envelope;
pub mod model;

pub use citation::*;
pub use envelope::*;
pub use model::*;
mod recovery;
pub use recovery::*;
mod added;
pub use added::added_date;

pub(crate) mod response;
mod schema;
pub use schema::{GoldenCase, schema};

mod draft;
pub use draft::*;
