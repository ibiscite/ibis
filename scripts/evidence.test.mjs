import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, readdir, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { prepareEvidence } from "./publish-evidence.mjs";

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
