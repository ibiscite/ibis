import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { gzipSync } from "node:zlib";
import { pickerBudget, PICKER_LIMIT } from "./picker-budget.mjs";

function build(t, manifest) {
  const root = mkdtempSync(join(tmpdir(), "ibis-picker-budget-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, ".vite"));
  writeFileSync(join(root, ".vite/manifest.json"), JSON.stringify(manifest));
  for (const file of ["picker.js", "shared.js", "picker.css", "settings.js", "lazy.js"])
    writeFileSync(join(root, file), `synthetic ${file}`);
  return root;
}

test("initial picker graph counts shared JS and CSS once and excludes other windows and lazy imports", (t) => {
  const root = build(t, {
    "index.html": { isEntry: true, file: "picker.js", imports: ["shared"], css: ["picker.css"], dynamicImports: ["lazy"] },
    shared: { file: "shared.js", css: ["picker.css"], imports: ["shared"] },
    "settings.html": { isEntry: true, file: "settings.js" },
    lazy: { file: "lazy.js" },
  });
  const report = pickerBudget(root);
  assert.deepEqual(report.files, ["picker.css", "picker.js", "shared.js"]);
  assert.equal(report.gzip_bytes, report.files.reduce((n, file) => n + gzipSync(`synthetic ${file}`, { level: 9 }).length, 0));
  assert.equal(PICKER_LIMIT, 409600);
  assert.doesNotThrow(() => pickerBudget(root, report.gzip_bytes));
  assert.throws(() => pickerBudget(root, report.gzip_bytes - 1), /exceed/);
});

test("missing entries, dependencies and assets fail closed", (t) => {
  for (const manifest of [
    {},
    { "index.html": { file: "picker.js" } },
    { "index.html": { isEntry: true, file: "missing.js" } },
    { "index.html": { isEntry: true, file: "picker.js", imports: ["missing"] } },
    { "index.html": { isEntry: true, file: "../outside.js" } },
  ]) assert.throws(() => pickerBudget(build(t, manifest)));
});
