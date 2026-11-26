//! Shared contracts and pure rules for sources, search and citation output.

pub mod atomic_file;
pub mod citable;
pub mod citation;
pub mod command_install;
pub mod contract;
pub mod destination;
pub mod epoch;
pub mod file_io;
pub mod identifier;
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
