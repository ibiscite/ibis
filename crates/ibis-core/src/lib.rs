//! Ibis core: sources, search and citation output, with no UI dependency.
//!
//! Everything that can be tested without a window lives here. The desktop
//! app, the `ibis` command line tool and future clients (MCP, browser
//! extension) are thin layers over this crate.

pub mod bibtex;
pub mod cite;
pub mod library;
pub mod model;
pub mod search;
