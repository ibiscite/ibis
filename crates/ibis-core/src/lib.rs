//! Ibis core: sources, search and citation output, with no UI dependency.
//!
//! Everything that can be tested without a window lives here. The desktop
//! app, the `ibis` command line tool and future clients (MCP, browser
//! extension) are thin layers over this crate.

pub mod bibtex;
pub mod cite;
pub mod find;
pub mod identifier;
pub mod library;
pub mod locator_query;
pub mod model;
pub mod search;
pub mod settings;
pub mod source;
pub mod zotero;
