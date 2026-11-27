//! Front ends reach the engine through its contract (decision 0028). The crate
//! graph is read from `cargo metadata`, as Cargo itself reads the manifests, so
//! a renamed, target-specific or inherited dependency cannot hide. A new crate
//! or a new edge must declare its place in the graph.
#![cfg(test)]
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    process::Command,
};

use serde_json::Value;

/// The picture in docs/architecture.md: each workspace member, by folder, and
/// the workspace crates it ships with. `[dev-dependencies]` and
/// `[build-dependencies]` are not part of what ships, so they are not read.
///
/// Shared capabilities belong in engine methods; clients use core contract types.
const LAYERS: [(&str, &[&str]); 10] = [
    ("crates/ibis-core", &[]),
    ("crates/ibis-csl", &["ibis-core"]),
    ("crates/ibis-sources", &["ibis-core"]),
    ("crates/ibis-engine", &["ibis-core", "ibis-csl", "ibis-sources"]),
    // The CLI also serves the built-in EndNote adapter in a child process.
    ("crates/ibis-cli", &["ibis-core", "ibis-engine", "ibis-sources"]),
    // Native paste for any front end; the host supplies the paste setting.
    ("crates/ibis-native", &["ibis-core"]),
    // The macOS C ABI and clipboard writer link no engine or library code.
    ("crates/ibis-native-ffi", &["ibis-core", "ibis-native"]),
    (
        "apps/desktop/src-tauri",
        &[
            "ibis-core",
            "ibis-desktop-macros",
            "ibis-engine",
            "ibis-native",
            "ibis-native-ffi",
        ],
    ),
    // Records each command's types for the generated bindings; ships nothing else.
    ("apps/desktop/src-tauri/macros", &[]),
    // The fuzzing harness ships nothing; it reads BibTeX as the engine does and
    // PDFs as the `ibis` extractor's child process does.
    ("fuzz", &["ibis", "ibis-core", "ibis-sources"]),
];

type Graph = BTreeMap<String, BTreeSet<String>>;

fn text<'a>(value: &'a Value, field: &str) -> &'a str {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("cargo metadata: no {field} in {value}"))
}

fn list<'a>(value: &'a Value, field: &str) -> &'a [Value] {
    value[field]
        .as_array()
        .unwrap_or_else(|| panic!("cargo metadata: no {field} list"))
}

/// Each workspace member's folder and the workspace crates it ships with, by
/// package name. A dependency's `name` is its package, even when renamed.
fn shipped_graph(metadata: &Value) -> Graph {
    let root = Path::new(text(metadata, "workspace_root"));
    let members: BTreeSet<&str> = list(metadata, "workspace_members")
        .iter()
        .map(|id| id.as_str().unwrap_or_else(|| panic!("cargo metadata: member {id}")))
        .collect();
    let packages: Vec<&Value> = list(metadata, "packages")
        .iter()
        .filter(|package| members.contains(text(package, "id")))
        .collect();
    let names: BTreeSet<&str> = packages.iter().map(|package| text(package, "name")).collect();
    packages
        .iter()
        .map(|package| {
            let manifest = Path::new(text(package, "manifest_path"));
            let folder = manifest
                .parent()
                .and_then(|folder| folder.strip_prefix(root).ok())
                .unwrap_or_else(|| panic!("{} is outside {}", manifest.display(), root.display()))
                .to_string_lossy()
                .replace('\\', "/");
            let shipped = list(package, "dependencies")
                .iter()
                .filter(|dependency| dependency["kind"].is_null())
                .map(|dependency| text(dependency, "name"))
                .filter(|name| names.contains(name))
                .map(str::to_owned)
                .collect();
            (folder, shipped)
        })
        .collect()
}

fn workspace_metadata() -> Value {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
            "--manifest-path",
        ])
        .arg(manifest)
        .output()
        .unwrap_or_else(|error| panic!("cargo metadata: {error}"));
    assert!(
        output.status.success(),
        "cargo metadata: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| panic!("cargo metadata: {error}"))
}

#[test]
fn each_crate_uses_only_the_layers_below_it() {
    let picture: Graph = LAYERS
        .iter()
        .map(|(folder, crates)| {
            (
                (*folder).to_owned(),
                crates.iter().map(|name| (*name).to_owned()).collect(),
            )
        })
        .collect();
    assert_eq!(
        shipped_graph(&workspace_metadata()),
        picture,
        "Every workspace crate is in LAYERS with exactly the crates it ships with. \
         A front end that needs more calls a new engine method instead (decision 0028)."
    );
}

/// `ibis-core` is the pure domain a WebAssembly reader can use: its shipped
/// code opens no file, process or socket, reads no environment or clock,
/// starts no thread, installs no panic hook and has no platform branch. These
/// files still do and are moving to the crates that use them; the list only
/// shrinks.
const CORE_IMPURE: [&str; 5] = [
    "atomic_file.rs",
    "command_install.rs",
    "destination/mod.rs",
    "file_io.rs",
    "panic_guard.rs",
];

/// `std` modules that reach the world. `net` is allowed only for address types.
const IMPURE_STD: [&str; 7] = ["env", "fs", "net", "panic", "process", "thread", "time"];
const PURE_NET: [&str; 3] = ["IpAddr", "Ipv4Addr", "Ipv6Addr"];
/// Crates that are only bindings to an operating system.
const SYSTEM_CRATES: [&str; 5] = ["libc", "nix", "objc2", "windows", "windows_sys"];
/// `cfg` keys that branch on the platform.
const PLATFORM_CFG: [&str; 4] = ["target_family", "target_os", "unix", "windows"];
/// Methods that ask the file system about a path.
const PATH_IO: [&str; 10] = [
    "canonicalize",
    "exists",
    "is_dir",
    "is_file",
    "is_symlink",
    "metadata",
    "read_dir",
    "read_link",
    "symlink_metadata",
    "try_exists",
];

/// Rust source as identifiers and punctuation, without comments or literals.
fn tokens(source: &str) -> Vec<String> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let at = |i: usize| chars.get(i).copied().unwrap_or('\0');
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '/' && at(i + 1) == '/' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && at(i + 1) == '*' {
            let mut depth = 0;
            loop {
                if at(i) == '/' && at(i + 1) == '*' {
                    depth += 1;
                    i += 2;
                } else if at(i) == '*' && at(i + 1) == '/' {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else if i >= chars.len() {
                    break;
                } else {
                    i += 1;
                }
            }
        } else if c == '"' {
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                i += if chars[i] == '\\' { 2 } else { 1 };
            }
            i += 1;
        } else if (c == 'r' || (c == 'b' && at(i + 1) == 'r')) && {
            let start = if c == 'b' { i + 2 } else { i + 1 };
            let mut j = start;
            while at(j) == '#' {
                j += 1;
            }
            at(j) == '"' && (j > start || at(start) == '"')
        } {
            let start = if c == 'b' { i + 2 } else { i + 1 };
            let mut j = start;
            while at(j) == '#' {
                j += 1;
            }
            let hashes = j - start;
            i = j + 1;
            while i < chars.len() && !(chars[i] == '"' && (1..=hashes).all(|k| at(i + k) == '#')) {
                i += 1;
            }
            i += 1 + hashes;
        } else if c == '\'' {
            if at(i + 1) == '\\' {
                i += 3;
                while i < chars.len() && chars[i] != '\'' {
                    i += 1;
                }
                i += 1;
            } else if at(i + 2) == '\'' {
                i += 3;
            } else {
                // A lifetime: its name follows as an identifier.
                i += 1;
            }
        } else if c.is_alphanumeric() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            // A byte string or byte character: `b` was its prefix.
            if word == "b" && (at(i) == '"' || at(i) == '\'') {
                continue;
            }
            out.push(word);
        } else if c == ':' && at(i + 1) == ':' {
            out.push("::".into());
            i += 2;
        } else {
            out.push(c.to_string());
            i += 1;
        }
    }
    out
}

/// The index just past the bracket that closes the one at `open`.
fn closing(tokens: &[String], open: usize) -> usize {
    let mut depth = 0usize;
    for (i, token) in tokens.iter().enumerate().skip(open) {
        match token.as_str() {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
    }
    tokens.len()
}

/// The tokens with every `#[cfg(test)]` item taken out.
fn shipped(tokens: &[String]) -> Vec<String> {
    let test_attribute = ["#", "[", "cfg", "(", "test", ")", "]"];
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if !tokens[i..].starts_with(&test_attribute.map(String::from)) {
            out.push(tokens[i].clone());
            i += 1;
            continue;
        }
        i += test_attribute.len();
        while tokens.get(i).is_some_and(|t| t == "#") {
            i = closing(tokens, i + 1);
        }
        while i < tokens.len() {
            match tokens[i].as_str() {
                ";" => {
                    i += 1;
                    break;
                }
                "{" => {
                    i = closing(tokens, i);
                    break;
                }
                "(" | "[" => i = closing(tokens, i),
                _ => i += 1,
            }
        }
    }
    out
}

/// What in these shipped tokens reaches the world.
fn impurities(tokens: &[String]) -> Vec<String> {
    let t = |i: usize| tokens.get(i).map_or("", String::as_str);
    let mut found = Vec::new();
    // A `std` module named at `i`, as `std::m` or inside `std::{…}`.
    let module = |i: usize, found: &mut Vec<String>| {
        if IMPURE_STD.contains(&t(i)) && !(t(i) == "net" && t(i + 1) == "::" && PURE_NET.contains(&t(i + 2))) {
            found.push(format!("std::{}", t(i)));
        }
    };
    for i in 0..tokens.len() {
        if t(i) == "std" && t(i + 1) == "::" {
            if t(i + 2) == "{" {
                let end = closing(tokens, i + 2);
                let mut depth = 0;
                for j in i + 2..end {
                    match t(j) {
                        "{" => depth += 1,
                        "}" => depth -= 1,
                        _ if depth == 1 && (t(j - 1) == "{" || t(j - 1) == ",") => module(j, &mut found),
                        _ => {}
                    }
                }
            } else {
                module(i + 2, &mut found);
            }
        }
        if SYSTEM_CRATES.contains(&t(i)) && t(i + 1) == "::" && t(i.wrapping_sub(1)) != "::" {
            found.push(format!("{}::", t(i)));
        }
        if (t(i) == "cfg" || t(i) == "cfg_attr") && (t(i + 1) == "(" || (t(i + 1) == "!" && t(i + 2) == "(")) {
            let open = if t(i + 1) == "(" { i + 1 } else { i + 2 };
            if let Some(key) = tokens[open..closing(tokens, open)]
                .iter()
                .find(|token| PLATFORM_CFG.contains(&token.as_str()))
            {
                found.push(format!("cfg({key})"));
            }
        }
        if t(i) == "." && PATH_IO.contains(&t(i + 1)) && t(i + 2) == "(" {
            found.push(format!(".{}()", t(i + 1)));
        }
    }
    found
}

/// Each shipped source file of the crate at `src`, from `lib.rs` down its
/// `mod` declarations, with what in it reaches the world.
fn shipped_files(src: &Path) -> BTreeMap<String, Vec<String>> {
    let mut files = BTreeMap::new();
    let mut queue = vec![src.join("lib.rs")];
    while let Some(file) = queue.pop() {
        let source = std::fs::read_to_string(&file).unwrap_or_else(|error| panic!("{}: {error}", file.display()));
        let code = shipped(&tokens(&source));
        let dir = match file.file_name().and_then(|name| name.to_str()) {
            Some("lib.rs" | "mod.rs") => file.parent().unwrap().to_path_buf(),
            _ => file.with_extension(""),
        };
        for (i, token) in code.iter().enumerate() {
            if token == "mod" && code.get(i + 2).is_some_and(|t| t == ";") {
                let name = &code[i + 1];
                let flat = dir.join(format!("{name}.rs"));
                queue.push(if flat.exists() {
                    flat
                } else {
                    dir.join(name).join("mod.rs")
                });
            }
        }
        let relative = file.strip_prefix(src).unwrap().to_string_lossy().replace('\\', "/");
        files.insert(relative, impurities(&code));
    }
    files
}

#[test]
fn core_does_no_io() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ibis-core/src");
    let files = shipped_files(&src);
    let allowed: BTreeSet<&str> = CORE_IMPURE.into_iter().collect();
    let impure: BTreeMap<&str, &Vec<String>> = files
        .iter()
        .filter(|(file, found)| !found.is_empty() && !allowed.contains(file.as_str()))
        .map(|(file, found)| (file.as_str(), found))
        .collect();
    assert!(
        impure.is_empty(),
        "ibis-core must stay pure; move this code to the crate that uses it: {impure:#?}"
    );
    let stale: Vec<&str> = allowed
        .iter()
        .copied()
        .filter(|file| files.get(*file).is_none_or(Vec::is_empty))
        .collect();
    assert!(
        stale.is_empty(),
        "No longer impure or gone; take it off CORE_IMPURE: {stale:?}"
    );
}

#[test]
fn the_purity_scan_sees_through_imports_and_skips_tests_and_text() {
    let scan = |source: &str| impurities(&shipped(&tokens(source)));
    for source in [
        "use std::fs;",
        "use std::{collections::HashMap, fs::File};",
        "use std::{io, process};",
        "fn f() { let _ = std::env::var(\"A\"); }",
        "use std::net::TcpStream;",
        "fn now() { std::time::Instant::now(); }",
        "#[cfg(unix)] fn f() {}",
        "const HERE: bool = cfg!(target_os = \"macos\");",
        "use nix::fcntl;",
        "fn f(p: &Path) -> bool { p.exists() }",
    ] {
        assert!(!scan(source).is_empty(), "missed: {source}");
    }
    for source in [
        "use std::{collections::HashMap, sync::Arc};",
        "use std::net::IpAddr;",
        "const VERSION: &str = env!(\"CARGO_PKG_VERSION\");",
        "/// Reads std::fs::File.\nfn f() {} // std::process\n/* std::env */",
        "const A: &str = \"std::fs\"; const B: &str = r#\"std::fs \"quoted\"\"#; const C: char = '\"';",
        "fn f<'a>(x: &'a str) -> [char; 2] { ['\\'', '{'] }",
        "#[cfg(test)]\nmod tests { use std::fs; #[test] fn t() { std::process::id(); } }",
        "#[cfg(test)]\nmod tests;",
    ] {
        assert_eq!(scan(source), Vec::<String>::new(), "flagged: {source}");
    }
}
