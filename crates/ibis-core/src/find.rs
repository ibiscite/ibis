//! Finding `.bib` files the user may want to open, for first-run setup.
//!
//! Only names and modification times are looked at; no file is opened. The
//! walk is bounded in depth, in entries visited and in results, skips
//! hidden and tool folders, and never follows links, so a huge or looping
//! folder tree cannot make it slow.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Folders below a starting folder that are searched.
pub const MAX_DEPTH: usize = 3;
/// Files returned at most.
pub const MAX_FOUND: usize = 20;
/// Directory entries looked at in each starting folder before stopping.
const MAX_VISITED: usize = 20_000;

/// Folders that hold tools, caches or other applications' data, never a
/// library someone keeps by hand.
const SKIPPED: [&str; 8] = [
    "node_modules",
    "Library",
    "AppData",
    "Applications",
    "target",
    "venv",
    "__pycache__",
    "site-packages",
];

/// A `.bib` file found on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    pub modified: Option<SystemTime>,
}

/// What a search found, and whether it stopped before looking everywhere.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Search {
    /// Newest first, at most [`MAX_FOUND`].
    pub files: Vec<Found>,
    /// A starting folder held more entries than are looked at, or more
    /// files were found than are returned.
    pub cut_short: bool,
}

/// `.bib` files below `roots`, newest first. Each starting folder has its
/// own budget, so a huge one does not hide files in the others.
pub fn bib_files(roots: &[PathBuf]) -> Search {
    search(roots, MAX_VISITED)
}

fn search(roots: &[PathBuf], budget: usize) -> Search {
    let mut files = Vec::new();
    let mut cut_short = false;
    for root in roots {
        let mut left = budget;
        walk(root, 0, &mut left, &mut files);
        if left == 0 {
            cut_short = true;
        }
    }
    files.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.path.cmp(&b.path)));
    files.dedup_by(|a, b| a.path == b.path);
    if files.len() > MAX_FOUND {
        files.truncate(MAX_FOUND);
        cut_short = true;
    }
    Search { files, cut_short }
}

/// Walk `folder`, spending one unit of `left` per entry; stops as soon as
/// it reaches zero, before opening another folder.
fn walk(folder: &Path, depth: usize, left: &mut usize, found: &mut Vec<Found>) {
    if *left == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    let mut folders = Vec::new();
    for entry in entries.flatten() {
        if *left == 0 {
            return;
        }
        *left -= 1;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        // The entry's own type: a link is neither followed nor listed.
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if depth < MAX_DEPTH && !SKIPPED.contains(&name) {
                folders.push(entry.path());
            }
        } else if kind.is_file()
            && Path::new(name)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("bib"))
        {
            let modified = entry.metadata().ok().and_then(|m| m.modified().ok());
            found.push(Found {
                path: entry.path(),
                modified,
            });
        }
    }
    folders.sort();
    for child in folders {
        walk(&child, depth + 1, left, found);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn touch(path: &Path, age_seconds: u64) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|e| panic!("{e}"));
        }
        fs::write(path, "@misc{synthetic}").unwrap_or_else(|e| panic!("{e}"));
        let when = SystemTime::now() - Duration::from_secs(age_seconds);
        fs::File::options()
            .write(true)
            .open(path)
            .and_then(|file| file.set_modified(when))
            .unwrap_or_else(|e| panic!("{e}"));
    }

    fn names(found: &[Found], root: &Path) -> Vec<String> {
        found
            .iter()
            .map(|f| {
                f.path
                    .strip_prefix(root)
                    .unwrap_or(&f.path)
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    }

    #[test]
    fn bib_files_are_found_newest_first_within_depth_skipping_hidden_and_tool_folders() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let root = dir.path();
        touch(&root.join("old.bib"), 300);
        touch(&root.join("thesis/refs.BIB"), 10);
        touch(&root.join("a/b/c/deep.bib"), 20);
        touch(&root.join("a/b/c/d/too-deep.bib"), 5);
        touch(&root.join(".hidden/secret.bib"), 1);
        touch(&root.join("node_modules/pkg/x.bib"), 1);
        touch(&root.join("notes.bibx"), 1);
        touch(&root.join("refs.bib.bak"), 1);
        fs::create_dir_all(root.join("folder.bib")).unwrap_or_else(|e| panic!("{e}"));
        let found = bib_files(&[root.to_path_buf(), root.join("missing")]).files;
        assert!(!bib_files(&[root.to_path_buf()]).cut_short);
        assert_eq!(names(&found, root), ["thesis/refs.BIB", "a/b/c/deep.bib", "old.bib"]);
    }

    #[test]
    fn results_are_capped_and_roots_listed_twice_count_once() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        for i in 0..MAX_FOUND + 5 {
            touch(&dir.path().join(format!("r{i:02}.bib")), i as u64 + 1);
        }
        let root = dir.path().to_path_buf();
        let search = bib_files(&[root.clone(), root]);
        assert!(search.cut_short);
        let found = search.files;
        assert_eq!(found.len(), MAX_FOUND);
        assert_eq!(names(&found[..2], dir.path()), ["r00.bib", "r01.bib"]);
    }

    #[cfg(unix)]
    #[test]
    fn links_are_not_followed_so_a_loop_cannot_hang() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let root = dir.path();
        touch(&root.join("real/refs.bib"), 1);
        std::os::unix::fs::symlink(root, root.join("real/loop")).unwrap_or_else(|e| panic!("{e}"));
        std::os::unix::fs::symlink(root.join("real/refs.bib"), root.join("linked.bib"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(names(&bib_files(&[root.to_path_buf()]).files, root), ["real/refs.bib"]);
    }

    #[test]
    fn a_full_folder_stops_its_own_search_only_and_says_so() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let (big, small) = (dir.path().join("big"), dir.path().join("small"));
        for i in 0..10 {
            touch(&big.join(format!("sub{i}/note.txt")), 1);
        }
        touch(&small.join("refs.bib"), 1);
        let search = search(&[big, small], 5);
        assert!(search.cut_short);
        assert_eq!(names(&search.files, dir.path()), ["small/refs.bib"]);
    }
}
