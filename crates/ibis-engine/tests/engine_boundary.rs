//! Front ends reach the engine through its contract (decision 0028). This test
//! fails when the crate graph changes or a front end starts calling a lower
//! crate in a new place, so the gap that #1260 closes can only get smaller.
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

/// The picture in docs/architecture.md: each crate and the Ibis crates it may
/// use. `[dev-dependencies]` are not part of what ships, so they are not read.
const LAYERS: [(&str, &[&str]); 7] = [
    ("crates/ibis-core", &[]),
    ("crates/ibis-zotero-local", &["ibis-core"]),
    ("crates/ibis-csl", &["ibis-core"]),
    ("crates/ibis-sources", &["ibis-core", "ibis-zotero-local"]),
    ("crates/ibis-engine", &["ibis-core", "ibis-csl", "ibis-sources"]),
    ("crates/ibis-cli", &["ibis-core", "ibis-engine"]),
    (
        "apps/desktop/src-tauri",
        &["ibis-core", "ibis-engine", "ibis-zotero-local"],
    ),
];

const FRONT_ENDS: [&str; 2] = ["crates/ibis-cli/src", "apps/desktop/src-tauri/src"];

/// Shared words and settings that front ends may use anywhere until #1260
/// moves them behind `ibis-engine`.
const VOCABULARY: [&str; 8] = [
    "VERSION",
    "cite",
    "contract",
    "destination",
    "settings",
    "source",
    "toggle",
    "zotero",
];

/// Direct calls to citation rules that #1260 removes. Delete a line when its
/// use goes; never add one. A new need belongs in an engine method.
const STILL_CALLED_DIRECTLY: [(&str, &str, &str); 14] = [
    ("crates/ibis-cli/src/doctor.rs", "ibis_core", "syntax"),
    ("crates/ibis-cli/src/human.rs", "ibis_core", "model"),
    ("crates/ibis-cli/src/human.rs", "ibis_core", "syntax"),
    ("crates/ibis-cli/src/lib.rs", "ibis_core", "locator"),
    ("crates/ibis-cli/src/lib.rs", "ibis_core", "syntax"),
    (
        "apps/desktop/src-tauri/src/library/commands.rs",
        "ibis_zotero_local",
        "LocalZotero",
    ),
    (
        "apps/desktop/src-tauri/src/library/engine_file.rs",
        "ibis_core",
        "locator",
    ),
    ("apps/desktop/src-tauri/src/library/search.rs", "ibis_core", "locator"),
    ("apps/desktop/src-tauri/src/library/search.rs", "ibis_core", "model"),
    ("apps/desktop/src-tauri/src/library/search.rs", "ibis_core", "search"),
    (
        "apps/desktop/src-tauri/src/library/state.rs",
        "ibis_zotero_local",
        "closed",
    ),
    ("apps/desktop/src-tauri/src/main.rs", "ibis_zotero_local", "closed"),
    ("apps/desktop/src-tauri/src/setup.rs", "ibis_core", "find"),
    (
        "apps/desktop/src-tauri/src/setup.rs",
        "ibis_zotero_local",
        "LocalZotero",
    ),
];

const LOWER_CRATES: [&str; 4] = ["ibis_core", "ibis_csl", "ibis_sources", "ibis_zotero_local"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn is_word(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// The Ibis crates a manifest ships with, by the folder each path points at, so
/// that renaming a dependency cannot hide it.
fn shipped_crates(manifest: &str) -> BTreeSet<String> {
    let mut shipping = false;
    let mut crates = BTreeSet::new();
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            shipping = line == "[dependencies]" || (line.starts_with("[target.") && line.ends_with(".dependencies]"));
        } else if shipping && line.contains("path") {
            let name = line.rsplit_once("ibis-").map(|(_, rest)| {
                let end = rest.find(|c: char| !is_word(c) && c != '-').unwrap_or(rest.len());
                format!("ibis-{}", &rest[..end])
            });
            crates.extend(name);
        }
    }
    crates
}

fn without_line_comments(source: &str) -> String {
    let lines = source.lines().map(|line| line.split("//").next().unwrap_or_default());
    lines.collect::<Vec<_>>().join("\n")
}

/// The first path segments named under `use crate::{…}`, nested groups skipped.
fn group_names(group: &str) -> Vec<String> {
    let mut depth = 0;
    let mut names = vec![String::new()];
    for character in group.chars() {
        match character {
            '{' => depth += 1,
            '}' if depth == 0 => break,
            '}' => depth -= 1,
            ',' if depth == 0 => names.push(String::new()),
            _ if depth == 0 => names.last_mut().into_iter().for_each(|name| name.push(character)),
            _ => {}
        }
    }
    names
        .iter()
        .map(|name| name.trim().split("::").next().unwrap_or_default().to_owned())
        .map(|name| name.split_whitespace().next().unwrap_or_default().to_owned())
        .filter(|name| !name.is_empty())
        .collect()
}

/// Every `(crate, first segment)` a Rust file names. A glob, an alias or a bare
/// crate name reports as `*`, because it can reach anything.
fn lower_paths(source: &str) -> Vec<(&'static str, String)> {
    let source = without_line_comments(source);
    let mut found = vec![];
    for name in LOWER_CRATES {
        for (start, _) in source.match_indices(name) {
            let before = source[..start].chars().next_back();
            let rest = &source[start + name.len()..];
            if before.is_some_and(is_word) || rest.chars().next().is_some_and(is_word) {
                continue;
            }
            match rest.strip_prefix("::") {
                Some(group) if group.starts_with('{') => {
                    found.extend(group_names(&group[1..]).into_iter().map(|segment| (name, segment)));
                }
                Some(path) => {
                    let segment: String = path.chars().take_while(|c| is_word(*c)).collect();
                    found.push((name, if segment.is_empty() { "*".to_owned() } else { segment }));
                }
                None => found.push((name, "*".to_owned())),
            }
        }
    }
    found
}

fn allowed(file: &str, name: &str, segment: &str) -> bool {
    (name == "ibis_core" && VOCABULARY.contains(&segment)) || STILL_CALLED_DIRECTLY.contains(&(file, name, segment))
}

fn violations(file: &str, source: &str) -> Vec<String> {
    lower_paths(source)
        .into_iter()
        .filter(|(name, segment)| !allowed(file, name, segment))
        .map(|(name, segment)| format!("{file} calls {name}::{segment}"))
        .collect()
}

/// Tests may build fixtures with lower crates. Exact endings, so that a file
/// such as `latest.rs` is still checked.
fn is_test_file(name: &str) -> bool {
    let stem = name.strip_suffix(".rs").unwrap_or(name);
    ["tests", "acceptance"].contains(&stem) || ["_tests", "_test", "_acceptance"].iter().any(|end| stem.ends_with(end))
}

fn rust_files(folder: &Path, files: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(folder).unwrap_or_else(|error| panic!("{}: {error}", folder.display()));
    for entry in entries {
        let path = entry
            .unwrap_or_else(|error| panic!("{}: {error}", folder.display()))
            .path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if path.is_dir() {
            rust_files(&path, files);
        } else if name.ends_with(".rs") && !is_test_file(&name) {
            files.push(path);
        }
    }
}

#[test]
fn each_crate_uses_only_the_layers_below_it() {
    for (folder, expected) in LAYERS {
        let path = root().join(folder).join("Cargo.toml");
        let manifest = fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let expected: BTreeSet<String> = expected.iter().map(|name| (*name).to_owned()).collect();
        assert_eq!(shipped_crates(&manifest), expected, "{folder}");
    }
}

#[test]
fn front_ends_call_no_lower_crate_in_a_new_place() {
    let root = root();
    let mut problems = vec![];
    for front_end in FRONT_ENDS {
        let mut files = vec![];
        rust_files(&root.join(front_end), &mut files);
        // A moved folder must fail here, never pass by finding nothing.
        assert!(files.len() > 5, "{front_end}: only {} Rust files", files.len());
        for path in files {
            let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let file = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            problems.extend(violations(&file, &source));
        }
    }
    assert!(
        problems.is_empty(),
        "{}\nAdd an engine method and call that instead (decision 0028).",
        problems.join("\n")
    );
}

#[test]
fn the_check_refuses_a_rule_reached_around_the_engine() {
    let file = "apps/desktop/src-tauri/src/picker.rs";
    for source in [
        "let text = ibis_core::syntax::cite(&snapshot, &request);",
        "use ibis_core::{contract::CitationRequest, syntax::cite};",
        "use ibis_core::{\n    contract::{self, ItemRef},\n    search as find,\n};",
        "use ibis_core::*;",
        "use ibis_core as rules;",
        "let library = ibis_sources::read(&path);",
        "use ibis_csl::Formatter;",
        "let reader = ibis_zotero_local::closed::read();",
    ] {
        assert_eq!(violations(file, source).len(), 1, "{source}");
    }
}

#[test]
fn the_check_keeps_shared_words_and_ignores_what_is_not_code() {
    let file = "apps/desktop/src-tauri/src/picker.rs";
    for source in [
        "use ibis_core::contract::ItemRef;",
        "use ibis_core::{cite::KeyFormat, contract::{self, ItemRef}, settings::CitationStyle};",
        "// ibis_core::syntax was here once",
        "let my_ibis_core_value = 1;",
        "ibis_engine::Engine::new()",
    ] {
        assert_eq!(violations(file, source), Vec::<String>::new(), "{source}");
    }
}

#[test]
fn only_the_listed_file_may_keep_a_direct_call() {
    let source = "use ibis_core::search::SearchResult;";
    let listed = "apps/desktop/src-tauri/src/library/search.rs";
    assert!(violations(listed, source).is_empty());
    assert_eq!(violations("apps/desktop/src-tauri/src/picker.rs", source).len(), 1);
}

#[test]
fn a_manifest_cannot_hide_a_crate_behind_a_rename() {
    let manifest = "[dependencies]\nrules = { path = \"../ibis-sources\", package = \"ibis-sources\" }\n\n\
        [target.'cfg(unix)'.dependencies]\nibis-csl = { path = \"../ibis-csl\" }\n\n\
        [dev-dependencies]\nibis = { path = \"../ibis-cli\" }\n";
    let found: Vec<_> = shipped_crates(manifest).into_iter().collect();
    assert_eq!(found, ["ibis-csl", "ibis-sources"]);
}

#[test]
fn only_real_test_files_are_skipped() {
    for name in [
        "tests.rs",
        "acceptance.rs",
        "windows_tests.rs",
        "notepad_test.rs",
        "zotero_acceptance.rs",
    ] {
        assert!(is_test_file(name), "{name}");
    }
    for name in ["latest.rs", "attest.rs", "contest_entry.rs", "picker.rs"] {
        assert!(!is_test_file(name), "{name}");
    }
}

#[test]
#[should_panic(expected = "does-not-exist")]
fn a_missing_folder_fails_instead_of_passing_empty() {
    rust_files(&root().join("does-not-exist"), &mut vec![]);
}
