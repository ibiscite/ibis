//! Shared contracts and pure rules for sources, search and citation output.

pub mod atomic_file;
pub mod citable;
pub mod citation;
pub mod command_install;
pub mod contract;
pub mod destination;
pub mod identifier;
pub mod library_match;
pub mod locator;
pub mod model;
pub mod panic_guard;
mod scan;
pub mod search;
pub mod shorthand;
pub mod source;
pub mod summary;
pub mod syntax;
pub mod toggle;
pub mod zotero;

/// The version this build reports. A nightly sets `IBIS_VERSION` at build time
/// to its own version and commit, such as `0.2.8-nightly.20261102120000+abc1234`.
pub const VERSION: &str = match option_env!("IBIS_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};
