//! Typed engine methods shared by the command line and desktop (ADR 0028).
pub mod agent_apps;
pub mod discovery;
mod method;
mod sync;
pub use method::{Method, Needs, Network, Row, Selection, methods};

pub mod cayw;

pub mod catalogue;

pub mod mcp;

/// Which format a citation uses (decision 0042).
pub mod format;
pub mod outputs;

pub mod settings;
pub mod settings_backup;
pub mod zotero_open;

pub mod protocol;

pub mod request;

pub mod rpc;

mod service;
/// Source adapters (decision 0056, step 4).
pub use ibis_sources::adapter as sources;
pub use ibis_sources::identity;
/// The PDF extractor's contract: what `ibis __extract-pdf` answers and the
/// bounds the engine checks it against (decision 0055).
pub use ibis_sources::pdf;
/// The file names a library chooser offers, and whether a dropped file is one.
pub use ibis_sources::{library_file_extensions, names_a_library};
pub use service::*;

/// The most references one citation or bibliography can hold.
pub use ibis_core::syntax::MAX_CITATION_ITEMS;

/// The words of a search with its filters taken off, so a front end can offer
/// to search without them.
pub use ibis_core::search::without_filters;

pub mod wire;

pub mod server;

pub mod recents;

pub mod citations;
pub mod library;

/// Library changes agents propose and the person confirms (decision 0055).
pub mod write;

/// Context for the app receiving a citation.
pub mod app_context;
