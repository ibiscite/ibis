import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { test } from "node:test";
import { canvasInputs, checkPortablePaths, crates, decide, plan, wantsFull } from "./ci-plan.mjs";

// The workspace's shape: core at the bottom, the desktop app and its macros apart.
const metadata = {
  workspace_root: "/work/ibis",
  workspace_default_members: ["core-id", "cli-id", "sources-id", "csl-id", "engine-id"],
  packages: [
    ["core-id", "ibis-core", "crates/ibis-core", []],
    ["cli-id", "ibis", "crates/ibis-cli", ["ibis-core", "ibis-engine", "ibis-sources"]],
    ["engine-id", "ibis-engine", "crates/ibis-engine", ["ibis-core", "ibis-csl", "ibis-sources"]],
    ["csl-id", "ibis-csl", "crates/ibis-csl", ["ibis-core"]],
    ["sources-id", "ibis-sources", "crates/ibis-sources", ["ibis-core"]],
    ["desktop-id", "ibis-desktop", "apps/desktop/src-tauri", ["ibis-core", "ibis-desktop-macros", "ibis-engine"]],
    ["macros-id", "ibis-desktop-macros", "apps/desktop/src-tauri/macros", []],
    ["fuzz-id", "ibis-fuzz", "fuzz", ["ibis", "ibis-core", "ibis-sources"]],
  ].map(([id, name, dir, deps]) => ({
    id, name, manifest_path: `/work/ibis/${dir}/Cargo.toml`,
    dependencies: [...deps.map((dep) => ({ name: dep, path: `/work/ibis/${dep}` })), { name: "serde" }],
  })),
};
const workspace = crates(metadata);
const selected = (paths) => { const { reason, ...jobs } = plan(paths, workspace); return jobs; };
const full = { rust: true, workspace: true, pictures: true, csl: true, full: true };

test("crates come from cargo metadata with their workspace dependencies", () => {
  assert.deepEqual(workspace.find((crate) => crate.name === "ibis-desktop"), { name: "ibis-desktop", dir: "apps/desktop/src-tauri", deps: ["ibis-core", "ibis-desktop-macros", "ibis-engine"], workspace: false });
  assert.equal(workspace.find((crate) => crate.name === "ibis-core").workspace, true);
  const windows = crates({ ...metadata, workspace_root: "C:\\work\\ibis", packages: [{ ...metadata.packages[0], manifest_path: "C:\\work\\ibis\\crates\\ibis-core\\Cargo.toml" }] });
  assert.equal(windows[0].dir, "crates/ibis-core");
});

test("documentation alone runs no Rust or picture job", () => {
  assert.deepEqual(selected(["docs/testing.md", "README.md", "docs/design/picker.md", "docs/decisions/0049-local-gate.md", ".agents/skills/verify/SKILL.md", ".claude/skills", ".github/pull_request_template.md"]), { rust: false, workspace: false, pictures: false, csl: false, full: false });
});

test("Windows rejects reserved names, forbidden characters and trailing dots or spaces", () => {
  for (const path of [
    "docs/aux.md", "docs/CON .notes.md", "docs/PrN.md", "docs/nul.md", "docs/COM¹/guide.md",
    "docs/LPT9/guide.md", "docs/CONIN$.md", "docs/CONOUT$.md", "docs/GIT~1/guide.md",
    "docs/guide./index.md", "docs/guide /index.md",
    ...[..."<>:\"\\|?*", "\u0001", "\u001f"].map((character) => `docs/guide${character}.md`),
  ]) {
    assert.throws(() => checkPortablePaths([path], "windows-2025"), /Non-portable filename/, path);
  }
  checkPortablePaths(["docs/auxiliary.md", "docs/COM10/guide.md", "docs/guide .md"], "windows-2025");
});

test("case-insensitive checkouts reject file, directory and file-directory collisions", () => {
  for (const paths of [
    ["docs/guide.md", "docs/Guide.md"],
    ["docs/guide/intro.md", "docs/Guide/install.md"],
    ["docs/guide.md", "docs/Guide.md/index.md"],
  ]) {
    checkPortablePaths(paths, "ubuntu-24.04");
    for (const system of ["macos-15", "windows-2025"]) {
      assert.throws(() => checkPortablePaths(paths, system), /Conflicting checkout paths/);
    }
  }
});

test("macOS rejects canonical name collisions and Git directory aliases", () => {
  assert.throws(() => checkPortablePaths(["docs/café.md", "docs/cafe\u0301.md"], "macos-15"), /Conflicting checkout paths/);
  assert.throws(() => checkPortablePaths(["docs/.g\u200dit/guide.md"], "macos-15"), /Non-portable filename/);
});

test("checkout components have a 255-byte limit on Unix and a 255-code-unit limit on Windows", () => {
  for (const system of ["ubuntu-24.04", "macos-15"]) {
    checkPortablePaths([`docs/${"é".repeat(126)}.md`], system);
    assert.throws(() => checkPortablePaths([`docs/${"é".repeat(126)}x.md`], system), /component exceeds 255 UTF-8 bytes/);
    assert.throws(() => checkPortablePaths([`docs/${"é".repeat(128)}/guide.md`], system), /component exceeds 255 UTF-8 bytes/);
  }
  assert.throws(() => checkPortablePaths([`docs/${"😀".repeat(127)}.md`], "windows-2025"), /component exceeds 255 UTF-16 code units/);
});

// Build a Markdown path of the given ASCII length with room in each component.
function markdownPath(length) {
  const prefix = `docs/${`${"a".repeat(199)}/`.repeat(Math.floor((length - 9) / 200))}`;
  return `${prefix}${"b".repeat(length - prefix.length - 3)}.md`;
}

test("Unix path limits count UTF-8 bytes including the hosted checkout root", () => {
  for (const [system, max, root] of [
    ["ubuntu-24.04", 4095, "/home/runner/work/ibis/ibis/"],
    ["macos-15", 1023, "/Users/runner/work/ibis/ibis/"],
  ]) {
    const atLimit = markdownPath(max - root.length);
    checkPortablePaths([atLimit], system);
    assert.throws(() => checkPortablePaths([atLimit.replace("b", "é")], system), new RegExp(`checkout path exceeds ${max} UTF-8 bytes`));
  }
});

test("Windows file and directory limits count UTF-16 code units including the hosted checkout root", () => {
  const root = "D:\\a\\ibis\\ibis\\";
  const file = markdownPath(259 - root.length);
  checkPortablePaths([file], "windows-2025");
  assert.throws(() => checkPortablePaths([file.replace("b", "😀")], "windows-2025"), /checkout path exceeds 259 UTF-16 code units/);
  const directory = "a".repeat(247 - root.length - "docs/".length);
  checkPortablePaths([`docs/${directory}/guide.md`], "windows-2025");
  assert.throws(() => checkPortablePaths([`docs/${directory}a/guide.md`], "windows-2025"), /checkout directory exceeds 247 UTF-16 code units/);
});

test("a PR must have portable filenames even for prose or a full plan", async () => {
  const longProsePath = `docs/${`${"a".repeat(140)}/`.repeat(8)}guide.md`;
  for (const [paths, error] of [
    [["docs/AUX.md"], /Non-portable filename/],
    [["docs/guide.md", "docs/Guide.md"], /Conflicting checkout paths/],
    [[longProsePath], /macos-15 checkout path exceeds 1023 UTF-8 bytes/],
  ]) {
    const docs = { treePaths: () => paths, changedPaths: () => paths, workspaceCrates: () => workspace, pictureInputs: async () => new Set() };
    for (const SELECTION of ["", "full"]) {
      await assert.rejects(decide({ EVENT: "pull_request", SELECTION }, docs), error);
    }
  }
});

test("a custom macOS runner always gets a real checkout", async () => {
  const read = { treePaths: () => ["docs/testing.md"] };
  const { reason, ...jobs } = await decide({ EVENT: "pull_request", MACOS_RUNNER_OVERRIDE: "true" }, read);
  assert.deepEqual(jobs, full);
  assert.match(reason, /custom checkout root/);
});

test("a desktop-only change keeps the app's tests on all three systems and skips the workspace tests", () => {
  assert.deepEqual(selected(["apps/desktop/src-tauri/src/copy.rs"]), { rust: true, workspace: false, pictures: false, csl: false, full: false });
  assert.deepEqual(selected(["apps/desktop/src-tauri/macros/src/lib.rs", "docs/design.md"]), { rust: true, workspace: false, pictures: false, csl: false, full: false });
});

test("a workspace crate change tests every crate that depends on it", () => {
  assert.deepEqual(selected(["crates/ibis-cli/tests/lookup.rs"]), { rust: true, workspace: true, pictures: false, csl: false, full: false });
  assert.match(plan(["crates/ibis-csl/src/lib.rs"], workspace).reason, /ibis, ibis-csl, ibis-desktop, ibis-engine, ibis-fuzz/);
  assert.doesNotMatch(plan(["crates/ibis-cli/src/lib.rs"], workspace).reason, /ibis-desktop/);
});

test("an editor plugin change runs the command line's tests, which run the plugin", () => {
  assert.deepEqual(selected(["plugins/pandoc/_extensions/ibis/ibis.lua", "plugins/obsidian/main.js"]), { rust: true, workspace: true, pictures: false, csl: false, full: false });
  assert.match(plan(["plugins/obsidian/main.js"], workspace).reason, /ibis, ibis-fuzz/);
});

test("the CSL suite runs for changes to the formatter and its dependencies", () => {
  for (const path of ["crates/ibis-core/src/lib.rs", "crates/ibis-csl/vendor/README.md", "crates/ibis-csl/tests/conformance/driver.js"]) {
    assert.deepEqual(selected([path]), { rust: true, workspace: true, pictures: false, csl: true, full: false }, path);
  }
  assert.equal(selected(["crates/ibis-engine/src/lib.rs"]).csl, false);
});

test("an interface change runs the pictures and the app that embeds it", () => {
  for (const path of ["apps/desktop/src/App.tsx", "apps/desktop/src/tokens.css", "apps/desktop/package.json", "apps/desktop/canvas/scenarios.ts"]) {
    assert.deepEqual(selected([path]), { rust: true, workspace: false, pictures: true, csl: false, full: false }, path);
  }
  assert.deepEqual(selected(["apps/desktop/src/App.tsx", "crates/ibis-engine/src/lib.rs"]), { rust: true, workspace: true, pictures: true, csl: false, full: false });
});

test("the canvas build can select pictures for a native asset it imports", async () => {
  const icon = "apps/desktop/src-tauri/icons/128x128.png";
  const read = {
    treePaths: () => [icon],
    changedPaths: () => [icon],
    workspaceCrates: () => workspace,
    pictureInputs: async (paths) => new Set(paths.filter((path) => path === icon)),
  };
  const { reason, ...jobs } = await decide({ EVENT: "pull_request" }, read);
  assert.deepEqual(jobs, { rust: true, workspace: false, pictures: true, csl: false, full: false });
  assert.match(reason, /is a canvas build input/);
});

// The web job installs Vite before running this suite. The native jobs do not.
test("Vite reports the icon imported by the settings canvas", {
  skip: !existsSync(new URL("../apps/desktop/node_modules/vite/package.json", import.meta.url)),
}, async () => {
  const icon = "apps/desktop/src-tauri/icons/128x128.png";
  const paths = [icon, "apps/desktop/src-tauri/src/copy.rs", "docs/testing.md"];
  assert.deepEqual(await canvasInputs(paths), new Set([icon]));
});

test("shared, generated, gate and unknown paths run the full suite", () => {
  for (const path of ["Cargo.lock", "Cargo.toml", "rust-toolchain.toml", ".cargo/config.toml", "crates/ibis-core/build.rs", "apps/desktop/src-tauri/build.rs",
    "crates/ibis-sources/Cargo.toml", "apps/desktop/src-tauri/Cargo.lock", "apps/desktop/rust-toolchain", "apps/desktop/.cargo/config.toml", "apps/desktop/src-tauri",
    "fixtures/cases/02-pandoc.json", ".github/workflows/ci.yml", "scripts/ci-plan.mjs", "scripts/csl-suite.mjs", "scripts/csl-bundle.mjs",
    "locales/en-US.ftl", "docs/cli.md", "apps/desktop/src/bindings.ts",
    "website/pages/home.html", "plugins/ibis/plugin.json", "brand/logo.svg", ".config/nextest.toml", "fuzz/src/lib.rs", "docs/macos-metrics.swift",
    "deny.toml", "clippy.toml", "version.txt", "flake.nix", "nix/package.nix", "a-new-folder/file.txt", "apps/desktop", "cratesish/x.rs"]) {
    const result = plan(["docs/testing.md", path], workspace);
    assert.equal(result.full, true, path);
    const { reason, ...jobs } = result;
    assert.deepEqual(jobs, full, path);
    if (!path.startsWith("fuzz/")) assert.ok(result.reason.includes(path), result.reason);
  }
  for (const paths of [[], undefined, [""], [7]]) assert.equal(plan(paths, workspace).full, true, JSON.stringify(paths));
});

test("non-PR events always select every job", async () => {
  for (const EVENT of ["push", "schedule", "workflow_dispatch"]) {
    const { reason, ...jobs } = await decide({ EVENT }, {});
    assert.deepEqual(jobs, full, EVENT);
  }
});

test("main, schedules, manual runs, the label and the rollback variable run everything", () => {
  for (const event of ["push", "schedule", "workflow_dispatch", "", undefined]) assert.notEqual(wantsFull({ event }), "");
  assert.equal(wantsFull({ event: "pull_request" }), "");
  assert.notEqual(wantsFull({ event: "pull_request", labels: ["ci:full"] }), "");
  assert.notEqual(wantsFull({ event: "pull_request", selection: "full" }), "");
  assert.equal(wantsFull({ event: "pull_request", labels: ["documentation"], selection: "" }), "");
});

test("unreadable selection inputs widen to the full suite", async () => {
  const docs = {
    treePaths: () => ["docs/testing.md"],
    changedPaths: () => ["docs/testing.md"],
    workspaceCrates: () => workspace,
    pictureInputs: async () => new Set(),
  };
  assert.equal((await decide({ EVENT: "pull_request", LABELS: "[]" }, docs)).full, false);
  assert.equal((await decide({ EVENT: "pull_request", LABELS: "not json" }, docs)).full, true);
  for (const LABELS of ["null", "{}", '"ci:full"', "[null]"]) {
    assert.deepEqual({ ...(await decide({ EVENT: "pull_request", LABELS }, docs)), reason: undefined }, { ...full, reason: undefined });
  }
  assert.equal((await decide({ EVENT: "pull_request", LABELS: '["ci:full"]' }, docs)).full, true);
  assert.equal((await decide({ EVENT: "push" }, docs)).full, true);
  for (const broken of [
    { ...docs, changedPaths: () => { throw new Error("git diff failed"); } },
    { ...docs, workspaceCrates: () => { throw new Error("cargo metadata failed"); } },
    { ...docs, pictureInputs: async () => { throw new Error("canvas build failed"); } },
  ]) {
    const result = await decide({ EVENT: "pull_request", LABELS: "[]" }, broken);
    assert.deepEqual({ ...result, reason: undefined }, { ...full, reason: undefined });
  }
});
