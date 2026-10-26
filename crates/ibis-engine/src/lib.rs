//! Typed engine methods shared by the command line and desktop (ADR 0028).
mod method;
pub use method::Method;

pub mod protocol;

pub mod request;

pub mod rpc;
