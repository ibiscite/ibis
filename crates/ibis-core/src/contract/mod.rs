//! Shared engine contract (ADR 0028). Front ends consume these same types.
//! Existing readers and front ends migrate in their own rebuild issues.

pub mod citation;
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
