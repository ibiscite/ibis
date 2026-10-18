//! Ibis desktop app: a thin Tauri shell over `ibis-core`.
//!
//! The window is the picker. All behaviour that can be tested without a
//! window lives in `ibis-core`; this file only translates between the web
//! interface and the core.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Mutex;

use ibis_core::bibtex;
use ibis_core::model::{LoadResult, Reference};
use ibis_core::search::Index;
use serde::Serialize;
use tauri::State;

/// The library currently open in the app.
#[derive(Default)]
struct Library {
    path: Option<PathBuf>,
    loaded: LoadResult,
}

#[derive(Serialize)]
struct LibraryStatus {
    path: Option<String>,
    references: usize,
    warnings: usize,
}

#[derive(Serialize)]
struct SearchResult {
    key: String,
    title: String,
    authors: String,
    year: Option<i32>,
    matched: Vec<String>,
}

impl SearchResult {
    fn from(reference: &Reference, matched: Vec<String>) -> Self {
        SearchResult {
            key: reference
                .citekey
                .clone()
                .unwrap_or_else(|| reference.id.native_key.clone()),
            title: reference.title_plain().unwrap_or_default(),
            authors: reference
                .authors
                .iter()
                .map(|n| {
                    if n.literal.is_some() {
                        n.display()
                    } else {
                        n.family.clone().unwrap_or_default()
                    }
                })
                .collect::<Vec<_>>()
                .join(", "),
            year: reference.year(),
            matched,
        }
    }
}

fn status(library: &Library) -> LibraryStatus {
    LibraryStatus {
        path: library.path.as_ref().map(|p| p.display().to_string()),
        references: library.loaded.references.len(),
        warnings: library.loaded.warnings.len(),
    }
}

/// Largest library file read; far above any real BibTeX library.
const MAX_LIBRARY_BYTES: u64 = 64 * 1024 * 1024;

/// Read a library file, accepting only regular files up to [`MAX_LIBRARY_BYTES`],
/// so a device or pipe can never exhaust memory or block the app.
fn read_library_file(path: &std::path::Path) -> Result<String, String> {
    use std::io::Read;
    let metadata = std::fs::metadata(path).map_err(|e| format!("cannot read the library file: {e}"))?;
    if !metadata.is_file() {
        return Err("the library must be a regular file".to_string());
    }
    if metadata.len() > MAX_LIBRARY_BYTES {
        return Err(format!(
            "the library file is larger than {} MiB",
            MAX_LIBRARY_BYTES / 1024 / 1024
        ));
    }
    let file = std::fs::File::open(path).map_err(|e| format!("cannot read the library file: {e}"))?;
    let mut text = String::new();
    file.take(MAX_LIBRARY_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|e| format!("cannot read the library file as UTF-8 text: {e}"))?;
    if text.len() as u64 > MAX_LIBRARY_BYTES {
        return Err("the library file grew while it was being read".to_string());
    }
    Ok(text)
}

/// Load a BibTeX/BibLaTeX file read-only and make it the open library.
#[tauri::command]
fn open_library(path: String, library: State<'_, Mutex<Library>>) -> Result<LibraryStatus, String> {
    let path = PathBuf::from(path);
    let text = read_library_file(&path)?;
    let source = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    let loaded = bibtex::load_str(&source.to_string_lossy(), &text);
    let mut library = library.lock().map_err(|_| "library state is unavailable".to_string())?;
    *library = Library {
        path: Some(path),
        loaded,
    };
    Ok(status(&library))
}

/// Search the open library. An empty query returns no results.
#[tauri::command]
fn search(query: String, limit: usize, library: State<'_, Mutex<Library>>) -> Result<Vec<SearchResult>, String> {
    let library = library.lock().map_err(|_| "library state is unavailable".to_string())?;
    if library.path.is_none() {
        return Err("no library is open".to_string());
    }
    let index = Index::new(&library.loaded.references);
    Ok(index
        .search(&query, limit.min(200))
        .into_iter()
        .map(|hit| {
            let matched = hit.matched.iter().map(|m| format!("{m:?}").to_lowercase()).collect();
            SearchResult::from(hit.reference, matched)
        })
        .collect())
}

fn main() {
    let app = tauri::Builder::default()
        .manage(Mutex::new(Library::default()))
        .invoke_handler(tauri::generate_handler![open_library, search])
        .setup(|app| {
            // Development convenience until settings (#2) exist: open the
            // library named by IBIS_LIBRARY, if set.
            if let Ok(path) = std::env::var("IBIS_LIBRARY") {
                let state = tauri::Manager::state::<Mutex<Library>>(app);
                if let Err(err) = open_library(path, state) {
                    eprintln!("ibis: {err}");
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!());
    if let Err(err) = app {
        eprintln!("ibis: the app could not start: {err}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_regular_files_are_read() {
        assert!(read_library_file(std::path::Path::new(".")).is_err());
        #[cfg(unix)]
        assert!(read_library_file(std::path::Path::new("/dev/zero")).is_err());
        let file = std::env::temp_dir().join(format!("ibis-read-{}.bib", std::process::id()));
        std::fs::write(&file, "@misc{k}").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(read_library_file(&file).as_deref(), Ok("@misc{k}"));
    }
}
