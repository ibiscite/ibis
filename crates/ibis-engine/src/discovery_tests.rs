use std::{fs, time::Duration};

use super::*;

fn bib_files(roots: &[PathBuf]) -> (Vec<PathBuf>, bool) {
    search(roots, 20_000, &Cancellation::default())
}

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

fn names(found: &[PathBuf], root: &Path) -> Vec<String> {
    found
        .iter()
        .map(|f| f.strip_prefix(root).unwrap_or(f).to_string_lossy().replace('\\', "/"))
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
    let found = bib_files(&[root.to_path_buf(), root.join("missing")]).0;
    assert!(!bib_files(&[root.to_path_buf()]).1);
    assert_eq!(names(&found, root), ["thesis/refs.BIB", "a/b/c/deep.bib", "old.bib"]);
}

#[test]
fn results_are_capped_and_roots_listed_twice_count_once() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    for i in 0..25 {
        touch(&dir.path().join(format!("r{i:02}.bib")), i as u64 + 1);
    }
    let root = dir.path().to_path_buf();
    let search = bib_files(&[root.clone(), root]);
    assert!(search.1);
    let found = search.0;
    assert_eq!(found.len(), 20);
    assert_eq!(names(&found[..2], dir.path()), ["r00.bib", "r01.bib"]);
}

#[cfg(unix)]
#[test]
fn links_are_not_followed_so_a_loop_cannot_hang() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let root = dir.path();
    touch(&root.join("real/refs.bib"), 1);
    std::os::unix::fs::symlink(root, root.join("real/loop")).unwrap_or_else(|e| panic!("{e}"));
    std::os::unix::fs::symlink(root.join("real/refs.bib"), root.join("linked.bib")).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(names(&bib_files(&[root.to_path_buf()]).0, root), ["real/refs.bib"]);
}

#[test]
fn a_full_folder_stops_its_own_search_only_and_says_so() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let (big, small) = (dir.path().join("big"), dir.path().join("small"));
    for i in 0..10 {
        touch(&big.join(format!("sub{i}/note.txt")), 1);
    }
    touch(&small.join("refs.bib"), 1);
    let search = search(&[big, small], 5, &crate::Cancellation::default());
    assert!(search.1);
    assert_eq!(names(&search.0, dir.path()), ["small/refs.bib"]);
}
