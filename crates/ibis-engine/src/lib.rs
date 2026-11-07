//! Typed engine methods shared by the command line and desktop (ADR 0028).
pub mod discovery;
mod method;
pub use method::Method;

pub mod cayw;

pub mod catalogue;

pub mod mcp;

pub mod outputs;

pub mod settings_backup;

pub mod protocol;

pub mod request;

pub mod rpc;

mod service;
pub use ibis_sources::identity;
pub use service::*;

/// The most references one citation or bibliography can hold.
pub use ibis_core::syntax::MAX_CITATION_ITEMS;

/// The words of a search with its filters taken off, so a front end can offer
/// to search without them.
pub use ibis_core::search::without_filters;

pub mod wire;

pub mod server;

pub mod recents;
