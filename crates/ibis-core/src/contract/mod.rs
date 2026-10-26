//! Shared engine contract (ADR 0028). Front ends consume these same types.
//! Existing readers and front ends migrate in their own rebuild issues.

pub mod citation;
pub mod envelope;
pub mod model;

pub use citation::*;
pub use envelope::*;
pub use model::*;

mod schema;
pub use schema::{GoldenCase, schema};
