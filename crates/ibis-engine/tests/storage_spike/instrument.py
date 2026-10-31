"""Build private stage probes in a disposable copy; never edit production files.

Every replacement must match once. No algorithms, limits, readers, integrity
checks or native process/locality checks are replaced. The sole access shim
selects a generated closed directory instead of discovering a real profile.
"""

import json
from pathlib import Path
import shutil
import subprocess

from fixtures import ROOT, digest


def build():
    overlay = ROOT / "target/stage-a-source"
    if overlay.exists():
        shutil.rmtree(overlay)
    overlay.mkdir(parents=True)
    # Preserve all workspace manifests and include_* resources. Dependencies and
    # build output remain local to the worktree; no packages are installed.
    for path in ROOT.iterdir():
        if path.name in {"target", "crates", ".git", "spike-results"}:
            continue
        (overlay / path.name).symlink_to(path, target_is_directory=path.is_dir())
    shutil.copytree(ROOT / "crates", overlay / "crates", ignore=shutil.ignore_patterns("__pycache__"))
    patches = []

    def replace(relative, before, after):
        path = overlay / relative
        original = path.read_text()
        if original.count(before) != 1:
            raise RuntimeError(f"probe anchor changed: {relative}: {before[:70]}")
        path.write_text(original.replace(before, after))
        patches.append({"file": relative, "before": before, "after": after})

    def entry(relative, anchor, stage, core=False):
        module = "crate" if core else "ibis_core"
        replace(relative, anchor, anchor + f'\n    let _storage_spike_span = {module}::storage_spike_probe::span("{stage}");')

    core = "crates/ibis-core/src/"
    sources = "crates/ibis-sources/src/"
    closed = "crates/ibis-zotero-local/src/closed/"
    lib = overlay / core / "lib.rs"
    lib.write_text(lib.read_text() + '\n#[doc(hidden)]\npub mod storage_spike_probe;\n')
    shutil.copyfile(Path(__file__).with_name("probe.rs"), overlay / core / "storage_spike_probe.rs")
    entry(sources + "file.rs", "pub fn read_library_file(path: &Path) -> Result<String, SourceError> {", "read_copy")
    entry(sources + "bibtex/mod.rs", ") -> (ParsedLibrary, bool) {", "parse")
    entry(sources + "bibtex/convert.rs", ") -> Record {", "normalize")
    entry(sources + "zotero/mod.rs", "pub fn map_item(library: LibraryId, item: &ZoteroItem) -> Result<MappedItem, MappingError> {", "normalize")
    for anchor in ["pub fn new(objects: Vec<LibraryObject>) -> Result<Self, ModelError> {",
                   "pub fn new_reusing(objects: Vec<LibraryObject>, previous: &Self, reused: &[usize]) -> Result<Self, ModelError> {",
                   "fn from_sorted(objects: Vec<Arc<SnapshotObject>>) -> Result<Self, ModelError> {"]:
        entry(core + "contract/model.rs", anchor, "snapshot_fingerprints", True)
    entry(core + "search/snapshot.rs", "fn build(snapshot: Arc<Snapshot>, previous: Option<&Self>) -> Self {", "index", True)
    entry(sources + "file/cache.rs", "pub fn hydrate(generation: &PreparedLibrary) -> Result<LoadedLibrary, StoreError> {", "hydrate")
    # Includes secure file creation, read/write loop and sync_all, but keeps
    # source validation, native helper checks and copy cleanup in residual.
    replace(closed + "private_copy.rs", "        for index in COPY_ORDER {",
            '        let storage_copy = ibis_core::storage_spike_probe::span("read_copy");\n        for index in COPY_ORDER {')
    replace(closed + "private_copy.rs", "        after_copy()?;", "        drop(storage_copy);\n        after_copy()?;")
    replace(closed + "database.rs", '    let check: String = db',
            '    let storage_quick = ibis_core::storage_spike_probe::span("quick_check");\n    let check: String = db')
    replace(closed + "database.rs", '    validate_table(&db, "version", &["schema", "version"])?;',
            '    drop(storage_quick);\n    let _storage_parse = ibis_core::storage_spike_probe::span("parse");\n    validate_table(&db, "version", &["schema", "version"])?;')
    # Live transport nested in refresh: parse is JSON decode + inventory fences,
    # ownership and raw item construction; read_copy is real HTTP I/O.
    replace("crates/ibis-zotero-local/src/live.rs", "        let mut budget = Budget {",
            '        let _storage_parse = ibis_core::storage_spike_probe::span("parse");\n        let mut budget = Budget {')
    entry("crates/ibis-zotero-local/src/lib.rs", "    fn get(&mut self, path: &str) -> Result<Response, TransportError> {", "read_copy")
    shim = '''
impl ZoteroSource {
    /// Spike-only explicit fixture selection; every closed-reader check runs.
    pub fn storage_spike_closed(&mut self, data: &Path, cache: &Path) -> Result<Vec<Arc<LoadedLibrary>>, ZoteroError> {
        let acquired = read_closed_at(data, cache, Arc::new(|| false))?;
        self.publish_closed(&acquired, &|| false)
    }
}
'''
    replace(sources + "zotero_source.rs", "\n#[cfg(test)]\n#[allow(clippy::unwrap_used)]\nmod tests {",
            shim + "\n#[cfg(test)]\n#[allow(clippy::unwrap_used)]\nmod tests {")
    # The get() anchor above is unique today. Assert probe-only changes remain
    # inspectable and hash original/modified sources in the result artifact.
    modified = sorted({patch["file"] for patch in patches} | {core + "lib.rs", sources + "zotero_source.rs"})
    provenance = {"patches": patches, "sources": {path: {"original_sha256": digest(ROOT / path), "probed_sha256": digest(overlay / path)} for path in modified}, "probe_sha256": digest(Path(__file__).with_name("probe.rs")), "closed_access_shim": "explicit generated directory -> read_closed_at -> publish_closed; native safety checks unchanged"}
    build_result = subprocess.run(["cargo", "rustc", "--offline", "--locked", "--release", "-p", "ibis-engine", "--test", "storage_spike", "--message-format=json", "--", "--cfg", "storage_spike_probe"],
                                  cwd=overlay, capture_output=True, text=True, check=False)
    if build_result.returncode:
        raise RuntimeError(build_result.stderr)
    artifacts = [json.loads(line) for line in build_result.stdout.splitlines()]
    binary = next(Path(item["executable"]) for item in artifacts if item.get("reason") == "compiler-artifact" and item["target"]["name"] == "storage_spike" and item.get("executable"))
    provenance["executable_sha256"] = digest(binary)
    provenance["measurement_test_sha256"] = digest(overlay / "crates/ibis-engine/tests/storage_spike.rs")
    provenance["runner_sha256"] = digest(ROOT / "crates/ibis-engine/tests/storage_spike.py")
    provenance["generator_sha256"] = digest(Path(__file__).with_name("fixtures.py"))
    return binary, provenance
