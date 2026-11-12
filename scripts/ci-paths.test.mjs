import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { classify, coreSystems, parseNameStatus } from "./ci-paths.mjs";

const full = { core: true, interface: true, native: true, scripts: true, web: true };
const appOnly = { core: false, interface: true, native: true, scripts: false, web: true };
const interfaceOnly = { core: false, interface: true, native: false, scripts: false, web: true };
const websiteOnly = { core: false, interface: false, native: false, scripts: false, web: true };
const scriptsOnly = { ...websiteOnly, scripts: true };
const docsOnly = { core: false, interface: false, native: false, scripts: false, web: false };
const row = (filename, previous_filename = null) => ({ filename, previous_filename, status: previous_filename ? "renamed" : "modified" });
const pr = (files, expected = files.length) => classify("pull_request", expected, files);

test("interface-only changes retain web checks without native jobs", () => {
  for (const path of ["apps/desktop/src/App.tsx", "apps/desktop/src/features/picker/PickerFooter.css", "apps/desktop/src/libraryWords.ts", "apps/desktop/src/icon.svg", "apps/desktop/index.html", "apps/desktop/notice.html", "apps/desktop/settings.html", "apps/desktop/canvas/parts.tsx", "apps/desktop/gallery/webkit.swift", "apps/desktop/vite.canvas.config.ts"]) {
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
  for (const path of ["apps/desktop/src-tauri/src/lib.rs", "apps/desktop/src-tauri/tauri.conf.json", "apps/desktop/package.json", "apps/desktop/vite.config.ts", "apps/desktop/README.md", "apps/desktop/other.html", "apps/desktop/canvas-old/x.ts"]) {
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

test("crate code reads the app or the website only where input changes still run it", () => {
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
    "crates/ibis-cli/tests/engine_cli.rs": "reads fixtures/website/sample.bib, a fixture that runs the crates above",
    "crates/ibis-cli/tests/website_transcripts.rs": "reads CLI example content and fixtures, which run the crates above",
    "crates/ibis-core/src/locator.rs": 'the locator abbreviation "apps" (appendices), not a path',
    "crates/ibis-engine/tests/engine_boundary.rs": "reads only the app's Cargo.toml, a crate input above",
    "crates/ibis-engine/tests/persistence_settings_backup.rs": "the settings file apps.json, not the repository's apps folder",
    "crates/ibis-engine/tests/settings_backup.rs": "the settings file apps.json in a temporary folder, not the repository's apps folder",
  };
  assert.deepEqual(readers, Object.keys(allowed), "a new reader must run when its inputs change, or widen `core` in ci-paths.mjs");
});

test("website-only changes keep web checks without native jobs", () => {
  for (const path of ["website/index.html", "website/site.js", "website/_headers"]) {
    assert.deepEqual(pr([row(path)]), websiteOnly, path);
  }
  assert.deepEqual(pr([row("website/site.js"), row("apps/desktop/src-tauri/src/lib.rs")]), appOnly);
  assert.equal(pr([row("websites/site.js")]).native, true, "only the website directory itself");
});

test("website CLI examples run the crates that verify their outputs", () => {
  for (const filename of ["website/content/terminal.txt", "website/content/writing-apps.json", "fixtures/website/sample.bib"]) {
    for (const status of ["added", "modified", "removed"]) assert.deepEqual(pr([{ filename, status }]), full, `${status}: ${filename}`);
    assert.deepEqual(pr([row("website/old.txt", filename)]), full, `renamed from ${filename}`);
    assert.deepEqual(pr([row(filename, "website/old.txt")]), full, `renamed to ${filename}`);
  }
});

test("only allowlisted documentation additions, edits and deletions take the fast path", () => {
  for (const status of ["added", "modified", "removed"]) {
    for (const filename of ["README.md", "ROADMAP.md", "docs/design/menu.md", "website/README.md", "apps/desktop/src/ui/README.md", "LICENSE", ".github/ISSUE_TEMPLATE/bug_report.yml", ".github/workflows/preview.yml", ".github/workflows/release-counts.yml"]) {
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
  for (const path of ["crates/ibis-desktop/src/lib.rs", "Cargo.toml", "scripts/smoke-native.py", "scripts/update-e2e.py", ".github/workflows/ci.yml", ".github/workflows/ci.yaml", ".github/workflows/nested/preview.yml", ".github/actions/setup/action.yml", "unknown/file", "docs/design-settings.json", "docs/design-settings.json.bak"]) {
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

test("Node and shell script changes run script tests on every system without building the app", () => {
  for (const path of ["scripts/land.sh", "scripts/update-feed.mjs", "scripts/land.test.mjs", "scripts/publish-nightly.sh"]) {
    assert.deepEqual(pr([row(path), row("docs/testing.md")]), scriptsOnly, path);
  }
  assert.deepEqual(pr([row("scripts/land.sh"), row("website/index.html")]), scriptsOnly);
  assert.deepEqual(pr([row("scripts/land.sh"), row("apps/desktop/src/App.tsx")]), { ...interfaceOnly, scripts: true });
  assert.deepEqual(pr([row("scripts/land.sh"), row("apps/desktop/src-tauri/src/lib.rs")]), { ...appOnly, scripts: true });
  // The native jobs run these, and CI's own scripts and check.mjs decide what runs.
  for (const path of ["scripts/ci-paths.mjs", "scripts/ci-changes.sh", "scripts/ci-ubuntu-deps.sh", "scripts/check.mjs", "scripts/gen.mjs", "docs/cli.md", "scripts/smoke-native.py", "scripts/check-offline.ps1", "scripts/update-gtk-probe.c", "scripts/update-macos.js", "scripts/lib/helper.mjs"]) {
    assert.deepEqual(pr([row(path)]), full, path);
  }
  assert.deepEqual(pr([row("scripts/land.sh", "scripts/smoke-native.py")]), full, "both sides of a rename");
});

test("Rust never reads a script, so script changes may skip the crates and the app", () => {
  const root = fileURLToPath(new URL("..", import.meta.url));
  const files = spawnSync("git", ["ls-files", "-z", "--", "*.rs"], { cwd: root, encoding: "utf8" }).stdout.split("\0").filter(Boolean);
  assert.ok(files.length > 50, "the Rust files are listed");
  const readers = files.filter((file) => /"[^"\n]*(?<![\w-])scripts(?![\w-])[^"\n]*"/.test(readFileSync(join(root, file), "utf8")));
  // Each file allowed to name the folder, and why that is safe.
  const allowed = {
    "crates/ibis-zotero-local/tests/real_zotero.rs": "an ignored test's message naming the script that runs it",
  };
  assert.deepEqual(readers, Object.keys(allowed), "a new reader must run on script changes too, or narrow `scriptFile` in ci-paths.mjs");
});

test("main verifies each pushed commit by its changed areas; daily and manual runs select everything", () => {
  assert.deepEqual(classify("push", 1, [row("docs/design.md")]), docsOnly);
  assert.deepEqual(classify("push", 1, [row("crates/ibis-core/src/lib.rs")]), full);
  for (const event of ["schedule", "workflow_dispatch", "unknown"]) assert.deepEqual(classify(event, 1, [row("README.md")]), full);
});

test("git's name-status list reads as GitHub's file list", () => {
  assert.deepEqual(parseNameStatus("M\0docs/a.md\0R100\0crates/old.rs\0apps/desktop/new.rs\0D\0gone.txt\0X\0odd\0"), [
    { filename: "docs/a.md", previous_filename: null, status: "modified" },
    { filename: "apps/desktop/new.rs", previous_filename: "crates/old.rs", status: "renamed" },
    { filename: "gone.txt", previous_filename: null, status: "removed" },
    { filename: "odd", previous_filename: null, status: "unknown" },
  ]);
  assert.deepEqual(classify("push", 1, parseNameStatus("X\0odd\0")), full);
  assert.equal(parseNameStatus(null), null);
});

test("the CLI emits only fixed decisions for valid, missing and malformed metadata", (t) => {
  const dir = mkdtempSync(join(tmpdir(), "ibis-ci-paths-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const file = join(dir, "synthetic.ndjson");
  for (const [contents, expected] of [[JSON.stringify(row("apps/desktop/src/研究.tsx")), "core=false\ninterface=true\nnative=false\nscripts=false\nweb=true\n"], ["private-資料: invalid metadata", "core=true\ninterface=true\nnative=true\nscripts=true\nweb=true\n"], ["", "core=true\ninterface=true\nnative=true\nscripts=true\nweb=true\n"], [null, "core=true\ninterface=true\nnative=true\nscripts=true\nweb=true\n"]]) {
    if (contents !== null) writeFileSync(file, contents);
    else rmSync(file);
    const result = spawnSync(process.execPath, [fileURLToPath(new URL("./ci-paths.mjs", import.meta.url))], { encoding: "utf8", env: { ...process.env, EVENT: "pull_request", EXPECTED: "1", FILES: file }, timeout: 5000 });
    assert.equal(result.status, 0);
    assert.equal(result.stdout, expected + `systems=${JSON.stringify(coreSystems("pull_request", contents === null ? undefined : 1, contents && contents.startsWith("{") ? [JSON.parse(contents)] : null))}\n`);
    assert.equal(result.stderr, "");
  }
  assert.deepEqual(pr([{ filename: "README.md" }]), full);
});

test("the crates' tests skip only when none of their inputs changed", () => {
  assert.deepEqual(pr([row("ROADMAP.md")]), docsOnly);
  assert.deepEqual(pr([row("ROADMAP.md", "crates/ibis-core/src/lib.rs")]), full);
});

test("interface selection identifies changes for the daily WebKit canvas", () => {
  for (const path of ["apps/desktop/src/tokens.css", "apps/desktop/src-tauri/src/lib.rs", "apps/desktop/gallery/webkit.swift", "crates/ibis-core/src/lib.rs"]) assert.equal(pr([row(path)]).interface, true, path);
  for (const path of ["website/index.html", "docs/design.md", "README.md"]) assert.equal(pr([row(path)]).interface, false, path);
});

// scripts/ci-changes.sh as CI runs it, with only GitHub's API and, for a crash,
// Node replaced by executables written before anything runs.
function changesFixture(t) {
  const dir = mkdtempSync(join(tmpdir(), "ibis-ci-changes-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  for (const name of ["github", "crash", "git"]) mkdirSync(join(dir, name));
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
  writeFileSync(join(dir, "git/git"), '#!/bin/sh\nif [ "$1" = fetch ]; then exit "$FETCH_STATUS"; fi\nif [ "$1" = diff ]; then printf "%b" "$DIFF_DATA"; fi\n', { mode: 0o755 });
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
    const expected = scenario === "valid" ? 'core=false\ninterface=true\nnative=false\nscripts=false\nweb=true\nsystems=["ubuntu-24.04"]\n' : 'core=true\ninterface=true\nnative=true\nscripts=true\nweb=true\nsystems=["ubuntu-24.04","windows-2025","macos-15"]\n';
    assert.equal(result.status, 0, scenario);
    assert.equal(result.stdout, expected, scenario);
    assert.equal(result.stderr, "", scenario);
    assert.equal(result.output, expected, `${scenario}: the step's outputs`);
  }
});

test("a classifier crash fails CI's file list step, even after partial output", { skip: process.platform === "win32" }, (t) => {
  const run = changesFixture(t);
  for (const partial of ["", "core=false\\ninterface=false\\nnative=false\\nscripts=false\\nweb=false\\n"]) {
    assert.equal(run(["github", "crash"], { EVENT: "schedule", PARTIAL: partial }).status, 17);
  }
});

test("Release Please's own updates wait for the release; anyone else's run is checked", { skip: process.platform === "win32" }, (t) => {
  const run = changesFixture(t);
  const release = { EVENT: "pull_request", HEAD_REF: "release-please--branches--main--components--ibis", REPO: "example/ibis", PR: "7", EXPECTED: "1", EXPECTED_REFS: `${"a".repeat(40)}:${"b".repeat(40)}`, SCENARIO: "valid", METADATA: JSON.stringify(row("Cargo.toml")) };
  const waiting = run(["github"], { ...release, ACTOR: "ibis-release[bot]" });
  assert.equal(waiting.status, 1);
  assert.match(waiting.stdout, /::error::.*gh workflow run ci\.yml --ref release-please--branches--main--components--ibis/);
  assert.equal(waiting.output, "", "no decisions, so nothing runs and CI result fails");
  for (const actor of ["diyanko", "ibis-release", ""]) {
    const checked = run(["github"], { ...release, ACTOR: actor });
    assert.equal(checked.status, 0, actor);
    assert.match(checked.output, /^native=true$/m, actor);
  }
  assert.equal(run(["github"], { ...release, HEAD_REF: "feature", ACTOR: "ibis-release[bot]" }).status, 0);
});

test("fast core suites include touched platform implementations, while qualification covers all three", () => {
  assert.deepEqual(coreSystems("pull_request", 1, [row("crates/ibis-core/src/lib.rs")]), ["ubuntu-24.04"]);
  assert.deepEqual(coreSystems("pull_request", 1, [row("crates/ibis-sources/src/windows.rs")]), ["ubuntu-24.04", "windows-2025"]);
  assert.deepEqual(coreSystems("push", 1, [row("crates/ibis-engine/src/macos/mod.rs")]), ["ubuntu-24.04", "macos-15"]);
  assert.equal(coreSystems("schedule", 1, [row("README.md")]).length, 3);
  assert.equal(coreSystems("workflow_dispatch", 1, [row("README.md")]).length, 3);
});

test("main push classification reads that push's diff and never reuses old evidence", { skip: process.platform === "win32" }, (t) => {
  const run = changesFixture(t);
  for (const [diff, expected] of [["M\\0docs/note.md\\0", false], ["M\\0crates/ibis-core/src/lib.rs\\0", true]]) {
    const result = run(["git"], { EVENT: "push", BASE_SHA: "a".repeat(40), HEAD_SHA: "b".repeat(40), FETCH_STATUS: "0", DIFF_DATA: diff });
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.output, new RegExp(`^core=${expected}$`, "m"));
  }
  const unavailable = run(["git"], { EVENT: "push", BASE_SHA: "a".repeat(40), HEAD_SHA: "b".repeat(40), FETCH_STATUS: "1", DIFF_DATA: "" });
  assert.equal(unavailable.status, 0);
  assert.match(unavailable.output, /^core=true$/m);
});
