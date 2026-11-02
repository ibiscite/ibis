//! How `ibis toggle` knows the program it started is Ibis and took the request.
//!
//! The command line creates an empty file and starts the app with
//! `toggle --receipt <file>`. The app writes [`RECEIPT`] into it before anything
//! else, whether it opens the picker itself or forwards the toggle to the copy
//! already running. Any other program leaves the file empty.
use std::io::Write;
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

/// Answer a toggle. Only an existing, empty regular file is written, so the
/// argument can never create or overwrite a file.
pub fn sign_receipt(path: &Path) -> std::io::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() != 0 {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    let mut file = std::fs::OpenOptions::new().write(true).open(path)?;
    file.write_all(RECEIPT)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    #[test]
    fn only_the_exact_receipt_arguments_name_a_file() {
        let named = args(&["ibis-desktop", "toggle", "--receipt", "/synthetic/receipt"]);
        assert_eq!(receipt_path(&named), Some(Path::new("/synthetic/receipt")));
        for other in [
            args(&["ibis-desktop", "toggle"]),
            args(&["ibis-desktop", "toggle", "--receipt"]),
            args(&["ibis-desktop", "toggle", "--receipt", ""]),
            args(&["ibis-desktop", "show", "--receipt", "/synthetic/receipt"]),
            args(&["ibis-desktop", "toggle", "--receipt", "/synthetic/receipt", "extra"]),
        ] {
            assert_eq!(receipt_path(&other), None);
        }
    }

    #[test]
    fn a_receipt_fills_only_an_existing_empty_file() {
        let dir = std::env::temp_dir().join(format!("ibis-receipt-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let empty = dir.join("empty");
        std::fs::write(&empty, b"").unwrap();
        sign_receipt(&empty).unwrap();
        assert_eq!(std::fs::read(&empty).unwrap(), RECEIPT);
        // Already answered, missing, or a folder: nothing is written.
        assert!(sign_receipt(&empty).is_err());
        assert_eq!(std::fs::read(&empty).unwrap(), RECEIPT);
        let missing = dir.join("missing");
        assert!(sign_receipt(&missing).is_err());
        assert!(!missing.exists());
        assert!(sign_receipt(&dir).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
