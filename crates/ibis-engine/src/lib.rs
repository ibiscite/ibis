//! Typed engine methods shared by the command line and desktop (ADR 0028).
pub mod discovery;
mod method;
pub use method::Method;

pub mod cayw;

pub mod catalogue;

pub mod mcp;

pub mod outputs;

pub mod settings;
pub mod settings_backup;

pub mod protocol;

pub mod request;

pub mod rpc;

mod service;
pub use ibis_sources::identity;

/// Use your styles, kept beside the stable app's settings whichever build runs,
/// as the library-id key is, so Ibis, Ibis Nightly and `ibis` share them.
pub fn use_saved_styles() {
    ibis_csl::use_styles_folder(identity::config_dir().map(|dir| dir.join("com.ibiscite.ibis").join("styles")));
}
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
