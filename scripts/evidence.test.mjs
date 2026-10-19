import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, readdir, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { evidenceSources, prepareEvidence } from "./publish-evidence.mjs";

test("combined artifact names are unique across attempts and never selected as platform inputs", async () => {
  const workflow = await readFile(new URL("../.github/workflows/ci.yml", import.meta.url), "utf8");
  const merge = workflow.match(/uses: actions\/upload-artifact\/merge@[^\n]+\n        with:\n([\s\S]*?)(?=\n(?:      -|  \w)|$)/)?.[1];
  assert.ok(merge, "The existing artifact merger must remain configured");
  const name = merge.match(/^          name: (.+)$/m)?.[1];
  const pattern = merge.match(/^          pattern: (.+)$/m)?.[1];
  assert.equal(pattern, "picker-${{ github.sha }}-*");
  const sha = "a".repeat(40);
  const expand = attempt => name.replaceAll("${{ github.sha }}", sha).replaceAll("${{ github.run_attempt }}", String(attempt));
  const first = expand(1), retry = expand(2);
  assert.notEqual(first, retry, "A retry must not collide with the preserved combined artifact");
  assert.equal(first, `picker-combined-${sha}-attempt-1`);
  assert.equal(retry, `picker-combined-${sha}-attempt-2`);
  const platformArtifacts = ["Linux", "macOS", "Windows"].map(os => `picker-${sha}-${os}`);
  const artifacts = [`picker-${sha}`, first, retry, ...platformArtifacts];
  // The configured input glob has only a literal prefix followed by '*'.
  const inputPrefix = pattern.replace("${{ github.sha }}", sha).slice(0, -1);
  assert.deepEqual(artifacts.filter(artifact => artifact.startsWith(inputPrefix)), platformArtifacts);
  assert.match(merge, /^          delete-merged: true$/m);
});

test("a flattened Windows retry updates only Windows and keeps prior OS evidence and retention", async () => {
  const temp = await mkdtemp(join(tmpdir(), "ibis-evidence-singleton-"));
  try {
    const root = join(temp, "branch"), input = join(temp, "evidence-input");
    const sha = "a".repeat(40);
    await mkdir(join(input, "gallery-Windows"), { recursive: true });
    await mkdir(join(input, "smoke-Windows"));
    await writeFile(join(input, "picker-Windows.png"), "fresh Windows capture");
    await writeFile(join(input, "gallery-Windows", "damaged-library.png"), "fresh gallery");
    await writeFile(join(input, "smoke-Windows", "smoke.json"), "excluded report");
    await writeFile(join(input, "startup-Windows.log"), "excluded log");
    // Mimic a retry after the successful OS inputs have already been removed.
    for (const os of ["Linux", "macOS"]) {
      await mkdir(join(root, sha, os), { recursive: true });
      await writeFile(join(root, sha, os, "keep.png"), `keep ${os}`);
    }
    await writeFile(join(root, "index.json"), JSON.stringify([{ sha, firstSeen: 1 }]));
    const sources = await evidenceSources({ input, sha });
    assert.deepEqual(sources, [{ source: input, os: "Windows" }]);
    for (const capture of sources) await prepareEvidence({ root, sha, ...capture, now: 99 });
    assert.equal(await readFile(join(root, sha, "Windows", "picker-Windows.png"), "utf8"), "fresh Windows capture");
    assert.equal(await readFile(join(root, sha, "Windows", "gallery-Windows", "damaged-library.png"), "utf8"), "fresh gallery");
    for (const os of ["Linux", "macOS"]) assert.equal(await readFile(join(root, sha, os, "keep.png"), "utf8"), `keep ${os}`);
    assert.deepEqual(await readdir(join(root, sha, "Windows")), ["gallery-Windows", "picker-Windows.png"]);
    assert.deepEqual(JSON.parse(await readFile(join(root, "index.json"), "utf8")), [{ sha, firstSeen: 1 }]);
  } finally { await rm(temp, { recursive: true, force: true }); }
});

test("the normal multi-platform artifact layout remains supported", async () => {
  const temp = await mkdtemp(join(tmpdir(), "ibis-evidence-wrapped-"));
  try {
    const sha = "b".repeat(40);
    for (const os of ["Linux", "macOS", "Windows"]) {
      const source = join(temp, `picker-${sha}-${os}`);
      await mkdir(source);
      await writeFile(join(source, `picker-${os}.png`), "synthetic screenshot");
    }
    const sources = await evidenceSources({ input: temp, sha });
    assert.deepEqual(sources.map(item => item.os).sort(), ["Linux", "Windows", "macOS"]);
    for (const { os, source } of sources) assert.equal(source, join(temp, `picker-${sha}-${os}`));
  } finally { await rm(temp, { recursive: true, force: true }); }
});

test("ambiguous, mixed-platform and malformed downloads cannot select a publication source", async () => {
  const temp = await mkdtemp(join(tmpdir(), "ibis-evidence-layout-"));
  const sha = "c".repeat(40);
  const layouts = [
    ["picker-Windows.png", "picker-Linux.png"],
    ["picker-Windows.png", `picker-${sha}-Windows/picker-Windows.png`],
    ["picker-Windows.png", "gallery-Linux/dark.png"],
    ["picker-Windows.png", "smoke-macOS/startup.log"],
    ["picker-unknown.png"],
    ["picker-Windows.png/child.png"],
    [`picker-${"d".repeat(40)}-Windows/picker-Windows.png`],
    [`picker-${sha}-Linux/gallery-Windows/dark.png`],
    [`picker-${sha}-Windows/picker-Windows.png`, "gallery-Windows/dark.png"],
    ["unlabelled.png"],
  ];
  try {
    for (let i = 0; i < layouts.length; i++) {
      const input = join(temp, String(i));
      for (const file of layouts[i]) {
        await mkdir(join(input, file, ".."), { recursive: true });
        await writeFile(join(input, file), "synthetic capture");
      }
      await assert.rejects(evidenceSources({ input, sha }), /layout|mixed platforms/);
    }
    await assert.rejects(evidenceSources({ input: temp, sha: "../main" }), /commit/);
    const fileRoot = join(temp, "regular-file");
    await writeFile(fileRoot, "not a directory");
    await assert.rejects(evidenceSources({ input: fileRoot, sha }), /type/);
  } finally { await rm(temp, { recursive: true, force: true }); }
});

test("source discovery rejects root, marker, nested and wrapped symlinks", async () => {
  const temp = await mkdtemp(join(tmpdir(), "ibis-evidence-links-"));
  const sha = "e".repeat(40);
  try {
    const outside = join(temp, "outside");
    await mkdir(outside);
    await writeFile(join(outside, "private.png"), "must remain outside publication");
    for (const kind of ["root", "marker", "nested", "wrapped"]) {
      const input = join(temp, kind);
      if (kind === "root") await symlink(outside, input);
      else {
        await mkdir(input);
        if (kind === "wrapped") await symlink(outside, join(input, `picker-${sha}-Windows`));
        else if (kind === "marker") await symlink(join(outside, "private.png"), join(input, "picker-Windows.png"));
        else {
          await writeFile(join(input, "picker-Windows.png"), "synthetic marker");
          await mkdir(join(input, "gallery-Windows"));
          await symlink(outside, join(input, "gallery-Windows", "nested"));
        }
      }
      await assert.rejects(evidenceSources({ input, sha }), /symbolic/);
    }
    assert.equal(await readFile(join(outside, "private.png"), "utf8"), "must remain outside publication");
  } finally { await rm(temp, { recursive: true, force: true }); }
});

test("retains the newest 50 commits, merges OS captures and excludes logs", async () => {
  const temp = await mkdtemp(join(tmpdir(), "ibis-evidence-"));
  try {
    const root = join(temp, "branch");
    const source = join(temp, "capture");
    await mkdir(source);
    await writeFile(join(source, "picker.png"), "synthetic screenshot");
    await writeFile(join(source, "startup.log"), "excluded diagnostic");
    for (let i = 1; i <= 52; i++) {
      await prepareEvidence({ root, source, sha: i.toString(16).padStart(40, "0"), os: "Linux", now: i });
    }
    const sha = (52).toString(16).padStart(40, "0");
    await prepareEvidence({ root, source, sha, os: "Windows", now: 100 });
    const index = JSON.parse(await readFile(join(root, "index.json"), "utf8"));
    assert.equal(index.length, 50);
    assert.equal(index.at(-1).sha, sha);
    assert.equal(index.at(-1).firstSeen, 52);
    assert.equal((await readdir(root)).filter(n => /^[a-f0-9]{40}$/.test(n)).length, 50);
    assert.deepEqual(await readdir(join(root, sha, "Linux")), ["picker.png"]);
    assert.deepEqual(await readdir(join(root, sha, "Windows")), ["picker.png"]);
  } finally { await rm(temp, { recursive: true, force: true }); }
});

test("rejects path traversal and capture symlinks before publishing", async () => {
  const temp = await mkdtemp(join(tmpdir(), "ibis-evidence-"));
  try {
    const source = join(temp, "capture");
    const root = join(temp, "branch");
    await mkdir(source);
    const options = { root, source, sha: "a".repeat(40), os: "Linux", now: 1 };
    await assert.rejects(prepareEvidence({ ...options, sha: "../main" }), /commit/);
    await assert.rejects(prepareEvidence({ ...options, os: "../main" }), /platform/);
    await writeFile(join(temp, "private.png"), "outside source");
    await symlink(join(temp, "private.png"), join(source, "picker.png"));
    await assert.rejects(prepareEvidence(options), /symbolic/);
  } finally { await rm(temp, { recursive: true, force: true }); }
});

test("rejects destination symlinks without deleting or writing outside the branch", async () => {
  const temp = await mkdtemp(join(tmpdir(), "ibis-evidence-"));
  try {
    const source = join(temp, "capture");
    const root = join(temp, "branch");
    const outside = join(temp, "outside");
    const sha = "b".repeat(40);
    await mkdir(source);
    await mkdir(root);
    await mkdir(join(outside, "Linux"), { recursive: true });
    await writeFile(join(source, "picker.png"), "synthetic screenshot");
    await writeFile(join(outside, "Linux", "keep.txt"), "must survive");
    await symlink(outside, join(root, sha));
    await assert.rejects(prepareEvidence({ root, source, sha, os: "Linux", now: 1 }), /symbolic/);
    assert.equal(await readFile(join(outside, "Linux", "keep.txt"), "utf8"), "must survive");
    assert.deepEqual(await readdir(join(outside, "Linux")), ["keep.txt"]);
  } finally { await rm(temp, { recursive: true, force: true }); }
});
