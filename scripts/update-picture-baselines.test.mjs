import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFile, mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";

test("baseline refresh replaces the PNG set only after a nonempty download succeeds", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "ibis-refresh-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const scripts = path.join(root, "scripts"), bin = path.join(root, "bin");
  const gallery = path.join(root, "apps/desktop/gallery"), baseline = path.join(gallery, "baselines");
  await Promise.all([mkdir(scripts), mkdir(bin), mkdir(baseline, { recursive: true })]);
  const script = path.join(scripts, "update-picture-baselines.sh");
  await copyFile(new URL("./update-picture-baselines.sh", import.meta.url), script);
  await writeFile(path.join(baseline, "old.png"), "old picture");
  await writeFile(path.join(baseline, "README.md"), "CI only");
  await writeFile(path.join(bin, "gh"), `#!/usr/bin/env bash
set -eu
[[ "$1 $2 $3 $4 $5 $6" == 'run download 123 --name picture-baselines --dir' ]]
case "$MOCK_REFRESH" in
  failure) exit 1;;
  empty) exit 0;;
  success) echo 'new picture' > "$7/new.png";;
esac
`, { mode: 0o755 });
  for (const mode of ["failure", "empty", "success"]) {
    const result = spawnSync("bash", [script.replaceAll("\\", "/"), "123"], { encoding: "utf8", timeout: 5000,
      env: { ...process.env, PATH: `${bin}${path.delimiter}${process.env.PATH}`, MOCK_REFRESH: mode } });
    assert.equal(result.status, mode === "success" ? 0 : 1, result.stderr);
    assert.deepEqual((await readdir(baseline)).sort(), ["README.md", mode === "success" ? "new.png" : "old.png"]);
    assert.equal(await readFile(path.join(baseline, "README.md"), "utf8"), "CI only");
    assert.deepEqual(await readdir(gallery), ["baselines"]);
  }
});
