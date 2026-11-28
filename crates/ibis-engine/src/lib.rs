//! The engine: one [`Engine`] owns the libraries, settings, citation sessions
//! and recents, and answers every operation through one method table
//! ([`methods`]), so the command line, MCP, the daemon and every app ask it the
//! same way (decisions 0028, 0057).
//!
//! The [layer](https://github.com/ibiscite/ibis/blob/main/docs/architecture.md#layers)
//! above the readers and the formatter. Hosts and apps use it; it uses none of
//! them. The methods' JSON is the versioned contract (`ibis schema`); the Rust
//! API may change before 1.0.
//!
//! Searching a library held in memory, through the same table every client uses:
//!
//! ```
//! use ibis_core::contract::{ItemRef, LibraryId, LibraryObject, Record, Snapshot};
//! use ibis_engine::{Cancellation, Engine, Method};
//! use serde_json::json;
//! use std::sync::Arc;
//!
//! let record = Record {
//!     id: ItemRef { library: LibraryId::digest(b"example library"), key: "work-1".into() },
//!     csl: serde_json::from_value(json!({"type": "book", "title": "Synthetic signals"}))?,
//!     citation_keys: vec![],
//!     aliases: vec![],
//!     warnings: vec![],
//!     identifiers: vec![],
//!     added_at: None,
//! };
//! let snapshot = Snapshot::new(vec![LibraryObject::Record(record)]).map_err(|error| format!("{error:?}"))?;
//! let engine = Engine::from_snapshot(Arc::new(snapshot));
//! let search = Method::parse("search").map_err(|error| format!("{error:?}"))?;
//! let reply = engine
//!     .dispatch(search, &json!({"query": "signals"}), &Cancellation::default())
//!     .map_err(|error| format!("{error:?}"))?;
//! assert_eq!(reply.data["hits"].as_array().map(Vec::len), Some(1));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
pub mod agent_apps;

use ibis_core::destination::System;
/// The system this build runs on; other Unix desktops count as Linux.
pub const SYSTEM: System = if cfg!(target_os = "macos") {
    System::Macos
} else if cfg!(windows) {
    System::Windows
} else {
    System::Linux
};
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

/// Per-app paste choices in `apps.json`, shared by every delivering process.
pub mod grants;

pub mod settings;
pub mod settings_backup;
/// Open an item's PDF in Zotero rather than the item, when it has one.
pub use ibis_sources::zotero_local::open as zotero_open;

pub mod protocol;

pub mod request;

pub mod rpc;

mod service;
pub use ibis_sources::identity;
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
