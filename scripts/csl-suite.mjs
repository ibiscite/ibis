#!/usr/bin/env node
// Fetch the CSL project's processor tests at the commit pinned in
// crates/ibis-csl/tests/conformance/suite.json, into that folder's `suite/`,
// for the conformance run (crates/ibis-csl/src/conformance.rs). Like the style
// bundle, the suite comes from GitHub only here, never at run time.
//
//   node scripts/csl-suite.mjs           fetch at the pin and check every file
//   node scripts/csl-suite.mjs --update  move the pin to upstream's newest commit
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { checkedFile, download, run, sha256 } from "./csl-bundle.mjs";

const FIXTURES = "processor-tests/humans";

/** The fixtures by file name and SHA-256, in name order. */
export function fixtureHashes(names, read) {
  return Object.fromEntries(names.filter((name) => name.endsWith(".txt")).sort().map((name) => {
    // Names reach file paths and the published report as written.
    if (!/^[A-Za-z0-9][A-Za-z0-9_.-]*\.txt$/.test(name)) throw new Error(`${JSON.stringify(name)}: not a fixture name`);
    return [name, sha256(read(name))];
  }));
}

async function main() {
  const root = join(dirname(fileURLToPath(import.meta.url)), "..");
  const folder = join(root, "crates/ibis-csl/tests/conformance");
  const manifestPath = join(folder, "suite.json");
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  const update = process.argv.includes("--update");
  if (update) {
    const [commit] = run("git", ["ls-remote", `https://github.com/${manifest.suite.repo}.git`, "HEAD"]).split(/\s+/);
    manifest.suite.commit = commit;
  }
  const work = mkdtempSync(join(tmpdir(), "ibis-csl-suite-"));
  try {
    const tree = join(await download(manifest.suite, work), FIXTURES);
    const files = fixtureHashes(readdirSync(tree), (name) => checkedFile(join(tree, name)));
    if (!update && JSON.stringify(files) !== JSON.stringify(manifest.files)) {
      throw new Error("the pinned commit's fixtures no longer match suite.json");
    }
    const suite = join(folder, "suite");
    rmSync(suite, { recursive: true, force: true });
    mkdirSync(suite);
    for (const name of Object.keys(files)) writeFileSync(join(suite, name), checkedFile(join(tree, name)));
    if (update) writeFileSync(manifestPath, `${JSON.stringify({ ...manifest, files }, null, 1)}\n`);
    console.log(`${Object.keys(files).length} fixtures at ${manifest.suite.commit}`);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
