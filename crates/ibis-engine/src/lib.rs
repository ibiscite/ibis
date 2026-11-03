//! Typed engine methods shared by the command line and desktop (ADR 0028).
mod method;
pub use method::Method;

pub mod mcp;

pub mod protocol;

pub mod request;

pub mod rpc;

mod service;
pub use service::*;

/// The most references one citation or bibliography can hold.
pub use ibis_core::syntax::MAX_CITATION_ITEMS;

pub mod wire;

pub mod server;
