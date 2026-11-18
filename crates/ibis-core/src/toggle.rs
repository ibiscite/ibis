//! How `ibis toggle` knows the program it started is Ibis and took the request.
//!
//! The command line creates an empty file and starts the app with
//! `toggle --receipt <file>`. The app writes [`RECEIPT`] into it before anything
//! else, whether it opens the picker itself or forwards the toggle to the copy
//! already running. Any other program leaves the file empty.
use std::path::Path;

/// The argument that names the receipt file, after `toggle`.
pub const RECEIPT_ARG: &str = "--receipt";
/// What the app writes, so an unrelated program cannot answer by accident.
pub const RECEIPT: &[u8] = b"ibis toggle\n";

/// The receipt file named by `toggle --receipt <file>`, if these are those arguments.
pub fn receipt_path(args: &[String]) -> Option<&Path> {
    match args {
        [_, command, flag, path] if command == "toggle" && flag == RECEIPT_ARG && !path.is_empty() => {
            Some(Path::new(path))
        }
        _ => None,
    }
}
