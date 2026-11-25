//! A folder of PDFs as a library (decision 0061). Its records are the
//! folder's own `ibis-references.bib`, read like any `.bib`; this module
//! names that file, lists the folder's PDFs, finds those no entry's `file`
//! names yet and writes the entry Ibis keeps for one. It never writes:
//! keeping is the engine's, through the proposal writer.

use crate::SourceError;
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

/// The file holding a folder's records, beside its PDFs.
pub const LIBRARY_FILE: &str = "ibis-references.bib";
/// Subfolders read below the folder itself.
pub const MAX_DEPTH: usize = 4;
/// PDFs listed in one folder; past this the listing is cut short.
pub const MAX_PDFS: usize = 5_000;
/// Folder entries looked at in one listing, files and folders alike.
const MAX_ENTRIES: usize = 50_000;

/// The library file `path` names: a folder's [`LIBRARY_FILE`], or the path
/// itself when it is not a folder.
pub fn library_file(path: &Path) -> PathBuf {
    if std::fs::metadata(path).is_ok_and(|metadata| metadata.is_dir()) {
        path.join(LIBRARY_FILE)
    } else {
        path.to_path_buf()
    }
}

/// `folder`, resolved, in the form the PDF reader's local-file checks take:
/// links resolved, and on Windows the plain `C:\` form of a `\\?\C:\` path.
pub fn readable(folder: &Path) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(folder).map(|resolved| crate::file::plain_disk(&resolved))
}

/// Whether the library file at `path` is a folder's, by its name alone.
pub fn is_folder_library(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == LIBRARY_FILE)
}

/// A folder's PDFs, by their paths relative to it with `/` between parts,
/// sorted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listing {
    pub pdfs: Vec<String>,
    /// Links, hidden entries, names that are not text or cannot sit in a
    /// BibTeX value, and folders past [`MAX_DEPTH`].
    pub skipped: usize,
    /// More PDFs or entries than one listing reads.
    pub cut_short: bool,
}

/// The PDFs in `folder` and its subfolders, never following a link.
pub fn list(folder: &Path, cancelled: &dyn Fn() -> bool) -> Result<Listing, SourceError> {
    let mut listing = Listing::default();
    let mut pending = vec![(folder.to_path_buf(), String::new(), 0usize)];
    let mut seen = 0usize;
    while let Some((dir, prefix, depth)) = pending.pop() {
        if cancelled() {
            return Err(SourceError::Cancelled);
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) if depth > 0 => {
                listing.skipped += 1;
                continue;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Err(SourceError::Missing),
            Err(_) => return Err(SourceError::Unavailable),
        };
        for entry in entries {
            seen += 1;
            if seen > MAX_ENTRIES || listing.pdfs.len() >= MAX_PDFS {
                listing.cut_short = true;
                pending.clear();
                break;
            }
            let Ok(entry) = entry else {
                listing.skipped += 1;
                continue;
            };
            let name = entry.file_name();
            let Some(name) = name.to_str().filter(|name| plain_name(name)) else {
                listing.skipped += 1;
                continue;
            };
            // `file_type` does not follow links, so a link is neither.
            let Ok(kind) = entry.file_type() else {
                listing.skipped += 1;
                continue;
            };
            let relative = if prefix.is_empty() {
                name.to_owned()
            } else {
                format!("{prefix}/{name}")
            };
            if kind.is_dir() {
                if depth < MAX_DEPTH {
                    pending.push((entry.path(), relative, depth + 1));
                } else {
                    listing.skipped += 1;
                }
            } else if kind.is_file() {
                if is_pdf_name(name) {
                    listing.pdfs.push(relative);
                }
            } else if kind.is_symlink() && is_pdf_name(name) {
                listing.skipped += 1;
            }
        }
    }
    listing.pdfs.sort();
    Ok(listing)
}

fn is_pdf_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
}

/// A name Ibis lists: not hidden, and one a BibTeX `file` value holds as
/// written, with no brace, backslash or control character.
fn plain_name(name: &str) -> bool {
    !name.starts_with('.') && !name.chars().any(|c| c.is_control() || matches!(c, '{' | '}' | '\\'))
}

/// The folder-relative paths the library's `file` fields name, with `/`
/// between parts, for those that stay inside the folder.
pub(crate) fn named(locations: impl IntoIterator<Item = String>) -> BTreeSet<String> {
    locations
        .into_iter()
        .filter_map(|location| relative(&location))
        .collect()
}

/// `location` as a plain relative path inside the folder, `/` between parts.
fn relative(location: &str) -> Option<String> {
    let path = Path::new(location);
    let mut parts = Vec::new();
    for part in path.components() {
        match part {
            Component::Normal(part) => parts.push(part.to_str()?),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// The folder library's id for its library file at `path`, which the
/// reader and the keeper both use.
pub fn library_id(path: &Path) -> ibis_core::contract::LibraryId {
    crate::identity::library_id(crate::LibraryKind::Folder, &[path.as_os_str().as_encoded_bytes()])
}

/// The folder-relative paths the `file` fields of the library text name.
pub fn kept_files(text: &str) -> BTreeSet<String> {
    let library = library_id(Path::new(LIBRARY_FILE));
    let parsed = crate::bibtex::load_with_budget(
        &library,
        text.trim_start_matches('\u{feff}'),
        crate::bibtex::Budget::for_input(text.len()),
        None,
    );
    let mut objects: Vec<_> = parsed
        .records
        .into_iter()
        .map(ibis_core::contract::LibraryObject::Record)
        .collect();
    let mut warnings = Vec::new();
    let references = crate::attachments::append(parsed.attachments, &mut objects, &mut warnings);
    named(references.into_iter().map(|(_, location)| location))
}

/// The PDFs in `listing` that no entry's `file` names.
pub fn unkept(listing: &Listing, named: &BTreeSet<String>) -> Vec<String> {
    listing
        .pdfs
        .iter()
        .filter(|pdf| !named.contains(*pdf))
        .cloned()
        .collect()
}

/// The `file` value for the PDF at `relative`: the form JabRef and Zotero
/// write, `:./path:application/pdf`, with `:` and `;` escaped. The `./`
/// keeps a name that starts with a space or looks like a drive or a scheme
/// a relative path when it is read back.
pub fn file_value(relative: &str) -> String {
    let mut path = String::with_capacity(relative.len() + 8);
    for c in relative.chars() {
        if matches!(c, ':' | ';') {
            path.push('\\');
        }
        path.push(c);
    }
    format!(":./{path}:application/pdf")
}

#[cfg(test)]
#[path = "folder_tests.rs"]
mod tests;
