// @vitest-environment node
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { onTestFinished, test } from "vitest";
import { fileURLToPath } from "node:url";
import { PNG } from "pngjs";

const script = fileURLToPath(new URL("./check-pictures.mjs", import.meta.url));
const picture = (changed = 0, value = 0, width = 100) => {
  const png = new PNG({ width, height: 100 });
  png.data.fill(255);
  for (let pixel = 0; pixel < changed; pixel++) png.data.fill(value, pixel * 4, pixel * 4 + 3);
  return PNG.sync.write(png);
};

test("picture gate compares decoded pixels, enforces tolerance and writes a readable diff", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "ibis-pictures-"));
  onTestFinished(() => rm(root, { recursive: true, force: true }));
  const [baseline, actual, diffs] = ["baseline", "actual", "diffs"].map((name) => path.join(root, name));
  await Promise.all([mkdir(baseline), mkdir(actual)]);
  const run = () => spawnSync(process.execPath, [script, baseline, actual, diffs], { encoding: "utf8", timeout: 5000 });
  let result = run();
  assert.equal(result.status, 1);
  assert.match(result.stderr, /empty.*update-picture-baselines/s);
  await writeFile(path.join(baseline, "screen.png"), picture());
  result = run();
  assert.equal(result.status, 1);
  assert.match(result.stderr, /No pictures were rendered/);

  // Exactly one in 10,000 pixels is allowed; two visibly changed pixels fail.
  for (const [changed, value, status] of [[0, 0, 0], [1, 0, 0], [2, 0, 1], [10000, 254, 0]]) {
    await writeFile(path.join(actual, "screen.png"), picture(changed, value));
    result = run();
    assert.equal(result.status, status, result.stderr);
    if (status) {
      assert.match(result.stderr, /2 pixels/);
      const diff = PNG.sync.read(await readFile(path.join(diffs, "screen.png")));
      assert.equal(diff.width, 100);
      assert.equal(diff.height, 100);
    }
  }
  await writeFile(path.join(actual, "screen.png"), picture(0, 0, 101));
  result = run();
  assert.equal(result.status, 1);
  assert.match(result.stderr, /dimensions/);
  assert.equal(PNG.sync.read(await readFile(path.join(diffs, "screen.png"))).width, 101);

  await rm(path.join(actual, "screen.png"));
  await writeFile(path.join(actual, "extra.png"), picture());
  result = run();
  assert.equal(result.status, 1);
  assert.match(result.stderr, /screen.png: baseline has no rendered picture/);
  assert.match(result.stderr, /extra.png: missing baseline/);

  await rm(path.join(actual, "extra.png"));
  await writeFile(path.join(actual, "screen.png"), "broken PNG");
  result = run();
  assert.equal(result.status, 1);
  assert.match(result.stderr, /screen.png:/);
});
