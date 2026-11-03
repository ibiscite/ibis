//! Typed engine methods shared by the command line and desktop (ADR 0028).
mod method;
pub use method::Method;

pub mod cayw;

pub mod mcp;

pub mod protocol;

pub mod request;

pub mod rpc;

mod service;
pub use service::*;

/// The words of a search with its filters taken off, so a front end can offer
/// to search without them.
pub use ibis_core::search::without_filters;

pub mod wire;

pub mod server;
