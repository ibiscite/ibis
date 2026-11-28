//! The nouns every Ibis layer speaks: records, libraries and snapshots
//! ([`contract`]), citation requests and results, errors with their one
//! recovery, and the rules that need nothing but data (search, citation key
//! syntaxes, locators, identifiers, [`destination`]).
//!
//! It is the bottom of the [layers](https://github.com/ibiscite/ibis/blob/main/docs/architecture.md#layers)
//! and never touches the world: no file, socket, process, clock, environment,
//! thread or platform branch, so it builds wherever Rust does
//! (`core_does_no_io` in `ibis-engine` holds it there). The JSON forms of
//! [`contract`] are the versioned contract; the Rust API may change before 1.0.
//!
//! A one-record library cited as Pandoc Markdown, with nothing but this crate:
//!
//! ```
//! use ibis_core::contract::{CitationRequest, ItemRef, LibraryId, LibraryObject, Record, Snapshot};
//! use serde_json::json;
//!
//! let id = ItemRef { library: LibraryId::digest(b"example library"), key: "work-1".into() };
//! let record = Record {
//!     id: id.clone(),
//!     csl: serde_json::from_value(json!({"type": "book", "title": "Synthetic signals"}))?,
//!     citation_keys: vec![serde_json::from_value(json!({"source": "bibtex", "value": "Taylor2026"}))?],
//!     aliases: vec![],
//!     warnings: vec![],
//!     identifiers: vec![],
//!     added_at: None,
//! };
//! let snapshot = Snapshot::new(vec![LibraryObject::Record(record)])?;
//! let request: CitationRequest = serde_json::from_value(json!({
//!     "items": [{"item": id}],
//!     "output": {"kind": "key", "syntax": "pandoc"},
//! }))?;
//! let cited = ibis_core::syntax::cite(&snapshot, &request).map_err(|error| format!("{error:?}"))?;
//! assert_eq!(cited.text, "[@Taylor2026]");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod citable;
pub mod citation;
pub mod contract;
pub mod destination;
pub mod epoch;
pub mod identifier;
pub mod locator;
pub mod model;
mod scan;
pub mod search;
pub mod shorthand;
pub mod source;
pub mod summary;
pub mod syntax;
pub mod zotero;

const BUILT: &str = match option_env!("IBIS_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};
const BUILD_FIELD: &[u8] = b"\nIbis-Build: ";
const BUILD_LEN: usize = BUILD_FIELD.len() + BUILT.len() + 1;
/// `\nIbis-Build: VERSION\n`, the one delimited copy of the version every
/// program that reports it carries, so a package check can find the whole
/// build and nothing longer (scripts/verify-packages.py).
const BUILD_MARK: [u8; BUILD_LEN] = {
    let mut mark = [b'\n'; BUILD_LEN];
    let mut at = 0;
    while at < BUILD_FIELD.len() {
        mark[at] = BUILD_FIELD[at];
        at += 1;
    }
    let version = BUILT.as_bytes();
    let mut at = 0;
    while at < version.len() {
        mark[BUILD_FIELD.len() + at] = version[at];
        at += 1;
    }
    mark
};

/// The version this build reports. A nightly sets `IBIS_VERSION` at build time
/// to its own version and commit, such as `0.2.8-nightly.20261102120000+abc1234`.
/// It is read out of [`BUILD_MARK`], so wherever it is used the binary holds
/// the whole delimited field.
pub const VERSION: &str = {
    let (_, rest) = BUILD_MARK.as_slice().split_at(BUILD_FIELD.len());
    let (version, _) = rest.split_at(BUILT.len());
    match std::str::from_utf8(version) {
        Ok(version) => version,
        Err(_) => panic!("IBIS_VERSION is not UTF-8"),
    }
};
