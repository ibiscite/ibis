#!/usr/bin/env node
// Download the pinned citation engine, styles and locale into
// apps/desktop/vendor/csl and check each against the SHA-256 in its
// manifest. Run by hand when updating a pin; the app never downloads them.
//
//   node scripts/vendor-citeproc.mjs           verify and (re)write the files
//   node scripts/vendor-citeproc.mjs --update  record new hashes for new pins

import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const dir = join(root, "apps/desktop/vendor/csl");
const manifestPath = join(dir, "manifest.json");
const update = process.argv.includes("--update");

const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
for (const file of manifest.files) {
  const source = manifest.sources[file.source];
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
