import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { classify } from "./ci-paths.mjs";

const full = { core: true, interface: true, native: true, web: true };
const appOnly = { core: false, interface: true, native: true, web: true };
const interfaceOnly = { core: false, interface: true, native: false, web: true };
const websiteOnly = { core: false, interface: false, native: false, web: true };
const docsOnly = { core: false, interface: false, native: false, web: false };
const row = (filename, previous_filename = null) => ({ filename, previous_filename, status: previous_filename ? "renamed" : "modified" });
const pr = (files, expected = files.length) => classify("pull_request", expected, files);

test("interface-only changes retain web checks without native jobs", () => {
  for (const path of ["apps/desktop/src/App.tsx", "apps/desktop/src/features/picker/PickerFooter.css", "apps/desktop/src/messages.ts", "apps/desktop/src/icon.svg", "apps/desktop/index.html"]) {
    assert.deepEqual(pr([row(path), row("docs/design.md")]), interfaceOnly, path);
  }
});

test("generated bindings always run native contract checks, including renames", () => {
  const bindings = "apps/desktop/src/bindings.ts";
  for (const status of ["added", "modified", "removed"]) {
    assert.deepEqual(pr([{ filename: bindings, status }]), appOnly, status);
  }
  assert.deepEqual(pr([row("apps/desktop/src/renamed.ts", bindings)]), appOnly);
  assert.deepEqual(pr([row(bindings, "apps/desktop/src/old.ts")]), appOnly);
  assert.deepEqual(pr([row("apps/desktop/src/tokens.css")]), interfaceOnly);
});

test("changes inside the app skip the crates, and anything the crates read does not", () => {
  for (const path of ["apps/desktop/src-tauri/src/lib.rs", "apps/desktop/src-tauri/tauri.conf.json", "apps/desktop/package.json", "apps/desktop/vite.config.ts", "apps/desktop/README.md"]) {
    assert.deepEqual(pr([row(path), row("apps/desktop/src/App.tsx")]), appOnly, path);
  }
  for (const path of ["crates/ibis-core/src/lib.rs", "fixtures/cases/a.json", "plugins/ibis/plugin.json", "Cargo.lock", "scripts/check.mjs", ".github/workflows/ci.yml"]) {
    assert.deepEqual(pr([row("apps/desktop/src-tauri/src/lib.rs"), row(path)]), full, path);
  }
  // The engine boundary test reads the app's manifest through cargo metadata.
  assert.deepEqual(pr([row("apps/desktop/src-tauri/Cargo.toml")]), full);
  assert.equal(pr([row("apps/desktop/src-tauri/Cargo.toml.orig")]).core, false);
  assert.equal(pr([row("apps/desktop-old/file.rs")]).core, true, "only the app directory itself");
  assert.equal(pr([row("apps/desktop/src-tauri/src/moved.rs", "crates/ibis-core/src/moved.rs")]).core, true);
});

// Any Rust string with the word "apps" or "website" in it, however a path is built.
const namesAppOrWebsite = (source) => /"[^"\n]*(?<![\w-])(?:apps|website)(?![\w-])[^"\n]*"/.test(source);

test("crate code reads the app or the website only where app changes still run it", () => {
  for (const source of ['root.join("apps").join("desktop/src/App.tsx")', 'concat!("apps", "/desktop/x")', '"../../website/index.html"', 'Path::new("apps/desktop")', 'format!("{root}/apps/desktop")', 'root.join("./apps")', 'PathBuf::from(format!("{root}/apps"))', '"website"']) {
    assert.ok(namesAppOrWebsite(source), source);
  }
  for (const source of ['"applications"', "// see `apps/desktop` for the parity tests", '"website-title"']) assert.ok(!namesAppOrWebsite(source), source);
  const root = fileURLToPath(new URL("..", import.meta.url));
  const files = spawnSync("git", ["ls-files", "-z", "--", "crates/*.rs", "fuzz/*.rs"], { cwd: root, encoding: "utf8" }).stdout.split("\0").filter(Boolean);
  assert.ok(files.length > 50, "the crates' Rust files are listed");
  const readers = files.filter((file) => namesAppOrWebsite(readFileSync(join(root, file), "utf8")));
  // Each file allowed to name these folders, and why that is safe.
  const allowed = {
    "crates/ibis-core/src/locator.rs": 'the locator abbreviation "apps" (appendices), not a path',
    "crates/ibis-core/tests/settings_backup.rs": "the settings file apps.json, not the repository's apps folder",
    "crates/ibis-engine/tests/engine_boundary.rs": "reads only the app's Cargo.toml, a crate input above",
    "crates/ibis-engine/tests/settings_backup.rs": "the settings file apps.json in a temporary folder, not the repository's apps folder",
  };
  assert.deepEqual(readers, Object.keys(allowed), "a new reader must run on app changes too, or widen `core` in ci-paths.mjs");
});

test("website-only changes keep web checks without native jobs", () => {
  for (const path of ["website/index.html", "website/site.js", "website/_headers"]) {
    assert.deepEqual(pr([row(path)]), websiteOnly, path);
  }
  assert.deepEqual(pr([row("website/site.js"), row("apps/desktop/src-tauri/src/lib.rs")]), appOnly);
  assert.equal(pr([row("websites/site.js")]).native, true, "only the website directory itself");
});

test("only allowlisted documentation additions, edits and deletions take the fast path", () => {
  for (const status of ["added", "modified", "removed"]) {
    for (const filename of ["README.md", "ROADMAP.md", "docs/design/menu.md", "website/README.md", "apps/desktop/src/ui/README.md", "LICENSE", ".github/ISSUE_TEMPLATE/bug_report.yml"]) {
      assert.deepEqual(pr([{ filename, status }]), docsOnly, `${status}: ${filename}`);
    }
  }
});

test("Markdown additions and deletions beside code and vendor assets require Rust checks", () => {
  for (const status of ["added", "modified", "removed"]) {
    for (const filename of ["crates/ibis-csl/vendor/README.md", "crates/ibis-csl/README.md", "fixtures/README.md", "vendor/README.md", "docs/vendor/README.md", "scripts/README.md", "unknown/README.md", "LICENSE/README.md"]) {
      assert.deepEqual(pr([{ filename, status }]), full, `${status}: ${filename}`);
    }
    assert.deepEqual(pr([{ filename: "website/vendor/README.md", status }]), websiteOnly);
  }
});

test("native, tooling and unknown changes stay conservative", () => {
  for (const path of ["crates/ibis-desktop/src/lib.rs", "Cargo.toml", "scripts/smoke-native.py", "scripts/update-e2e.py", ".github/workflows/ci.yml", "unknown/file", "docs/design-settings.json", "docs/design-settings.json.bak"]) {
    assert.deepEqual(pr([row("apps/desktop/src/App.tsx"), row(path)]), full, path);
  }
  assert.deepEqual(pr([row("apps/desktop/src/unknown.rs")]), appOnly);
});

test("both sides of a rename determine the required checks", () => {
  assert.deepEqual(pr([row("apps/desktop/src/Renamed.tsx", "apps/desktop/src/App.tsx")]), interfaceOnly);
  assert.equal(pr([row("docs/old.md", "crates/ibis-desktop/src/lib.rs")]).native, true);
  assert.deepEqual(pr([row("docs/vendor-notes.md", "crates/ibis-csl/vendor/README.md")]), full);
  assert.deepEqual(pr([row("crates/ibis-csl/vendor/README.md", "docs/vendor-notes.md")]), full);
  assert.deepEqual(pr([{ filename: "README.md", status: "renamed" }]), full);
});

test("empty, incomplete, malformed or ambiguous file lists run everything", () => {
  for (const files of [[], null, {}, [null], [row(12)], [row("")], [row("/README.md")], [row("../README.md")], [row("apps/desktop/src/../native.md")], [row("README.md\n")], [row("docs/\u0000.md")], [row("apps\\desktop\\src\\App.tsx")], [row("README.md", 42)], [row("README.md"), row("README.md")]]) {
    assert.deepEqual(pr(files, 1), full);
  }
  for (const expected of [0, -1, 2, "1", NaN, 3001]) assert.deepEqual(pr([row("README.md")], expected), full);
  assert.deepEqual(pr(Array.from({ length: 3001 }, (_, i) => row(`docs/${i}.md`))), full);
});

test("Unicode paths work without interpreting markup or shell syntax", () => {
  assert.deepEqual(pr([row("apps/desktop/src/研究.tsx"), row("apps/desktop/src/مرحبا.css"), row("docs/$(false)`x`.md")]), interfaceOnly);
});

test("main and merge queue always validate the complete app", () => {
  for (const event of ["push", "merge_group", "unknown"]) {
    for (const status of ["added", "modified", "removed"]) {
      for (const filename of ["README.md", "docs/design.md", "crates/ibis-csl/vendor/README.md"]) {
        assert.deepEqual(classify(event, 1, [{ filename, status }]), full);
      }
    }
  }
  assert.deepEqual(classify("workflow_dispatch"), full);
});

test("the CLI emits only fixed decisions for valid, missing and malformed metadata", (t) => {
  const dir = mkdtempSync(join(tmpdir(), "ibis-ci-paths-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const file = join(dir, "synthetic.ndjson");
  for (const [contents, expected] of [[JSON.stringify(row("apps/desktop/src/研究.tsx")), "core=false\ninterface=true\nnative=false\nweb=true\n"], ["private-資料: invalid metadata", "core=true\ninterface=true\nnative=true\nweb=true\n"], ["", "core=true\ninterface=true\nnative=true\nweb=true\n"], [null, "core=true\ninterface=true\nnative=true\nweb=true\n"]]) {
    if (contents !== null) writeFileSync(file, contents);
    else rmSync(file);
    const result = spawnSync(process.execPath, [fileURLToPath(new URL("./ci-paths.mjs", import.meta.url))], { encoding: "utf8", env: { ...process.env, EVENT: "pull_request", EXPECTED: "1", FILES: file }, timeout: 5000 });
    assert.equal(result.status, 0);
    assert.equal(result.stdout, expected);
    assert.equal(result.stderr, "");
  }
  assert.deepEqual(pr([{ filename: "README.md" }]), full);
});

test("the crates' tests skip only when none of their inputs changed", () => {
  assert.deepEqual(pr([row("ROADMAP.md")]), docsOnly);
  assert.deepEqual(pr([row("ROADMAP.md", "crates/ibis-core/src/lib.rs")]), full);
});

test("every interface change reaches the system's own WebKit on macOS", () => {
  for (const path of ["apps/desktop/src/tokens.css", "apps/desktop/src-tauri/src/lib.rs", "apps/desktop/gallery/webkit.swift", "crates/ibis-core/src/lib.rs"]) assert.equal(pr([row(path)]).interface, true, path);
  for (const path of ["website/index.html", "docs/design.md", "README.md"]) assert.equal(pr([row(path)]).interface, false, path);
});

// scripts/ci-changes.sh as CI runs it, with only GitHub's API and, for a crash,
// Node replaced by executables written before anything runs.
function changesFixture(t) {
  const dir = mkdtempSync(join(tmpdir(), "ibis-ci-changes-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  for (const name of ["github", "crash"]) mkdirSync(join(dir, name));
  writeFileSync(join(dir, "github/gh"), `#!/usr/bin/env bash
if [[ "$*" == *"/files"* ]]; then
  printf '%s\n' "$METADATA"
  [[ "$SCENARIO" != failedfiles ]]
else
  if [[ "$SCENARIO" == changedrefs && -f "$RUNNER_TEMP/read" ]]; then printf 'changed\n'; else printf '%s\n' "$EXPECTED_REFS"; fi
  touch "$RUNNER_TEMP/read"
  [[ "$SCENARIO" != failedhead ]]
fi
`, { mode: 0o755 });
  writeFileSync(join(dir, "crash/node"), "#!/usr/bin/env bash\nprintf '%b' \"$PARTIAL\"\nexit 17\n", { mode: 0o755 });
  return (stubs, env) => {
    rmSync(join(dir, "read"), { force: true });
    writeFileSync(join(dir, "output"), "");
    const result = spawnSync(fileURLToPath(new URL("./ci-changes.sh", import.meta.url)), [], {
      cwd: fileURLToPath(new URL("..", import.meta.url)), encoding: "utf8", timeout: 5000,
      env: { ...process.env, PATH: [...stubs.map((name) => join(dir, name)), process.env.PATH].join(delimiter), RUNNER_TEMP: dir, GITHUB_OUTPUT: join(dir, "output"), ...env },
    });
    return { ...result, output: readFileSync(join(dir, "output"), "utf8") };
  };
}

test("CI's file list falls back to every check on failed reads or changing refs", { skip: process.platform === "win32" }, (t) => {
  const run = changesFixture(t);
  for (const scenario of ["valid", "changedrefs", "failedhead", "failedfiles", "partial", "malformed"]) {
    // The real Node runs the real classifier.
    const result = run(["github"], { EVENT: "pull_request", REPO: "example/ibis", PR: "7", EXPECTED: scenario === "partial" ? "2" : "1", EXPECTED_REFS: `${"a".repeat(40)}:${"b".repeat(40)}`, SCENARIO: scenario, METADATA: scenario === "malformed" ? "invalid-private-資料" : JSON.stringify(row("apps/desktop/src/App.tsx")) });
    const expected = scenario === "valid" ? "core=false\ninterface=true\nnative=false\nweb=true\n" : "core=true\ninterface=true\nnative=true\nweb=true\n";
    assert.equal(result.status, 0, scenario);
    assert.equal(result.stdout, expected, scenario);
    assert.equal(result.stderr, "", scenario);
    assert.equal(result.output, expected, `${scenario}: the step's outputs`);
  }
});

test("a classifier crash fails CI's file list step, even after partial output", { skip: process.platform === "win32" }, (t) => {
  const run = changesFixture(t);
  for (const partial of ["", "core=false\\ninterface=false\\nnative=false\\nweb=false\\n"]) {
    assert.equal(run(["github", "crash"], { EVENT: "merge_group", PARTIAL: partial }).status, 17);
  }
});
