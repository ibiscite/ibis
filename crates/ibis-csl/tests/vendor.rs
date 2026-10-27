#![allow(clippy::unwrap_used)]
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, path::Path};

// The embedded engine, styles and locale are exactly the pinned upstream files,
// and nothing unpinned sits beside them.
#[test]
fn vendored_files_match_their_pins() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor");
    let manifest: serde_json::Value = serde_json::from_slice(&fs::read(dir.join("manifest.json")).unwrap()).unwrap();
    let suffix = manifest["moduleSuffix"].as_str().unwrap().as_bytes();
    let mut pinned = BTreeSet::from(["README.md".to_owned(), "manifest.json".to_owned()]);
    for file in manifest["files"].as_array().unwrap() {
        let out = file["out"].as_str().unwrap();
        let bytes = fs::read(dir.join(out)).unwrap();
        let upstream = if file["module"].as_bool() == Some(true) {
            bytes
                .strip_suffix(suffix)
                .unwrap_or_else(|| panic!("{out} lacks the module suffix"))
        } else {
            &bytes
        };
        let hash: String = Sha256::digest(upstream)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(hash, file["sha256"].as_str().unwrap(), "{out}");
        pinned.insert(out.to_owned());
    }
    let mut present = BTreeSet::new();
    for entry in fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().into_string().unwrap();
        if entry.file_type().unwrap().is_dir() {
            for inner in fs::read_dir(entry.path()).unwrap() {
                present.insert(format!("{name}/{}", inner.unwrap().file_name().into_string().unwrap()));
            }
        } else {
            present.insert(name);
        }
    }
    assert_eq!(present, pinned);
}
