//! Front ends reach the engine through its contract (decision 0028). The crate
//! graph is read from `cargo metadata`, as Cargo itself reads the manifests, so
//! a renamed, target-specific or inherited dependency cannot hide. A new crate
//! or a new edge fails here, so the gap that #1260 closes can only get smaller.
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    process::Command,
};

use serde_json::{Value, json};

/// The picture in docs/architecture.md: each workspace member, by folder, and
/// the workspace crates it ships with. `[dev-dependencies]` and
/// `[build-dependencies]` are not part of what ships, so they are not read.
///
/// The front ends' `ibis-core` is the direct call that #1260 removes: delete a
/// name when its use goes; never add one. A new need belongs in an engine method.
const LAYERS: [(&str, &[&str]); 9] = [
    ("crates/ibis-core", &[]),
    ("crates/ibis-zotero-local", &["ibis-core"]),
    ("crates/ibis-csl", &["ibis-core"]),
    ("crates/ibis-sources", &["ibis-core", "ibis-zotero-local"]),
    ("crates/ibis-engine", &["ibis-core", "ibis-csl", "ibis-sources"]),
    ("crates/ibis-cli", &["ibis-core", "ibis-engine"]),
    (
        "apps/desktop/src-tauri",
        &["ibis-core", "ibis-desktop-macros", "ibis-engine"],
    ),
    // Records each command's types for the generated bindings; ships nothing else.
    ("apps/desktop/src-tauri/macros", &[]),
    // The fuzzing harness ships nothing; it reads BibTeX as the engine does.
    ("fuzz", &["ibis-core", "ibis-sources"]),
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

/// Invented metadata in the shape `cargo metadata --format-version 1 --no-deps` prints.
fn invented(app_dependencies: Value) -> Value {
    let package = |name: &str, folder: &str, dependencies: Value| {
        json!({
            "id": format!("path+file:///work/{folder}#{name}@0.1.0"),
            "name": name,
            "manifest_path": format!("/work/{folder}/Cargo.toml"),
            "dependencies": dependencies,
        })
    };
    let packages = [
        package("ibis-core", "crates/ibis-core", json!([])),
        package("ibis-sources", "crates/ibis-sources", json!([])),
        package("ibis-csl", "crates/ibis-csl", json!([])),
        package("ibis", "crates/ibis-cli", json!([])),
        package("ibis-desktop", "apps/desktop/src-tauri", app_dependencies),
    ];
    let members: Vec<Value> = packages.iter().map(|package| package["id"].clone()).collect();
    json!({ "workspace_root": "/work", "workspace_members": members, "packages": packages })
}

#[test]
fn a_manifest_cannot_hide_a_crate_behind_a_rename_or_a_platform() {
    let dependency = |name: &str, kind: Value, rename: Value, target: Value| json!({ "name": name, "kind": kind, "rename": rename, "target": target });
    let graph = shipped_graph(&invented(json!([
        dependency("ibis-sources", Value::Null, "rules".into(), Value::Null),
        dependency("ibis-csl", Value::Null, Value::Null, "cfg(unix)".into()),
        dependency("ibis", "dev".into(), Value::Null, Value::Null),
        dependency("ibis-core", "build".into(), Value::Null, Value::Null),
        dependency("serde", Value::Null, Value::Null, Value::Null),
    ])));
    let app: Vec<_> = graph["apps/desktop/src-tauri"].iter().map(String::as_str).collect();
    assert_eq!(app, ["ibis-csl", "ibis-sources"]);
    // A member with no workspace dependency is still in the graph, so a new
    // crate cannot pass unlisted.
    assert!(graph["crates/ibis-cli"].is_empty());
    assert_eq!(graph.len(), 5);
}
