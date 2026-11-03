//! Shared contracts and pure rules for sources, search and citation output.

pub mod citable;
pub mod cite;
pub mod contract;
pub mod destination;
pub mod find;
pub mod identifier;
pub mod locator;
pub mod model;
pub mod panic_guard;
mod scan;
pub mod search;
pub mod settings;
pub mod source;
pub mod syntax;
pub mod toggle;
pub mod zotero;

/// The version this build reports. A nightly sets `IBIS_VERSION` at build time
/// to its own version and commit, such as `0.2.8-nightly.20261102120000+abc1234`.
pub const VERSION: &str = match option_env!("IBIS_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};
