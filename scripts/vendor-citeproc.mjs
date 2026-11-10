#!/usr/bin/env node
// Download the pinned citation engine into crates/ibis-csl/vendor and check it
// against the SHA-256 in its manifest. Run by hand when updating the pin; the app
// never downloads it. scripts/csl-bundle.mjs does the same for styles and locales.
//
//   node scripts/vendor-citeproc.mjs           verify and (re)write the files
//   node scripts/vendor-citeproc.mjs --update  record new hashes for new pins
//   node scripts/vendor-citeproc.mjs --newest  move each pin to the newest commit
//                                              that changed its file (the styles workflow)

import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const dir = join(root, "crates/ibis-csl/vendor");
const manifestPath = join(dir, "manifest.json");
const newest = process.argv.includes("--newest");
const update = newest || process.argv.includes("--update");

/** The newest commit on the default branch that changed `path`. */
async function newestCommit(repo, path) {
  const headers = { accept: "application/vnd.github+json" };
  if (process.env.GH_TOKEN) headers.authorization = `Bearer ${process.env.GH_TOKEN}`;
  const url = `https://api.github.com/repos/${repo}/commits?path=${encodeURIComponent(path)}&per_page=1`;
  const response = await fetch(url, { headers });
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
  const sha = (await response.json())?.[0]?.sha;
  if (!/^[0-9a-f]{40}$/.test(sha ?? "")) throw new Error(`${url}: no commit`);
  return sha;
}

const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
for (const file of manifest.files) {
  const source = manifest.sources[file.source];
  if (newest) source.commit = await newestCommit(source.repo, file.path);
  const url = `https://raw.githubusercontent.com/${source.repo}/${source.commit}/${file.path}`;
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
  const bytes = Buffer.from(await response.arrayBuffer());
  const sha256 = createHash("sha256").update(bytes).digest("hex");
  if (update) file.sha256 = sha256;
  else if (sha256 !== file.sha256) throw new Error(`${file.path}: expected ${file.sha256}, got ${sha256}`);
  // The engine is a classic script; the module copy only adds an export.
  const out = file.module ? Buffer.concat([bytes, Buffer.from(manifest.moduleSuffix)]) : bytes;
  await mkdir(dirname(join(dir, file.out)), { recursive: true });
  await writeFile(join(dir, file.out), out);
  console.log(`${file.out} ${sha256}`);
}
if (update) await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
