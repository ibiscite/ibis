//! The open library: loading off the main thread, the search command and
//! choosing or reloading a library file or the Zotero library.
//!
//! - `error`: why loading or a command failed, as values.
//! - `source`: where references come from and how a source is named.
//! - `state`: the open library and the numbered requests that replace it.
//! - `load`: reading a file or Zotero into a new library, off the main thread.
//! - `retry`: when Zotero is checked again or a failed Zotero choice retried.
//! - `status`: what the interface and tray are told about the library.
//! - `search`: searching an index snapshot and the rows sent for it.
//! - `tasks`: background work and publishing status through the app.
//! - `commands`: the commands the interface calls.

pub(crate) mod commands;
pub(crate) mod engine_file;
mod error;
mod load;
pub(crate) mod open_source;
mod retry;
#[cfg(test)]
mod samples;
mod search;
mod source;
mod state;
mod status;
mod tasks;

pub(crate) use error::{CommandError, LoadError};
pub(crate) use search::{PreviewIntent, PreviewRequests, SearchRequests};
pub(crate) use source::Source;
pub(crate) use state::{Library, LoadRequests};
pub(crate) use status::{LibraryState, LibraryStatus};
pub(crate) use tasks::{
    accepts_library_file, choose_library_file, current_status, refresh_settings_notice, refresh_zotero,
    reload_library_file, retry_zotero_on_focus, start_open, use_zotero_library,
};
