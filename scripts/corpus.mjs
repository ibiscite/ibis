#!/usr/bin/env node
// Engineering-only CC0 corpus tooling. No product code imports this module.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const manifestPath = new URL("../corpus/manifest.json", import.meta.url);
const licenseUrl = "https://www.wikidata.org/wiki/Wikidata:Licensing";
const types = { book: "book", "article-journal": "article" };
const fields = ["id", "type", "title", "author", "issued", "DOI", "language", "container-title", "volume", "issue", "page", "URL"];

export function validateManifest(manifest) {
  assert.equal(manifest.schema_version, 1);
  assert.equal(manifest.license, "CC0-1.0", "only confirmed CC0 metadata is allowed");
  assert.equal(manifest.license_url, licenseUrl);
  assert.match(manifest.retrieved, /^\d{4}-\d{2}-\d{2}$/);
  assert.equal(new Date(manifest.retrieved).toISOString().slice(0, 10), manifest.retrieved);
  assert.ok(manifest.retrieved <= new Date().toISOString().slice(0, 10), "retrieval date is in the future");
  assert.ok(manifest.entities.length > 0 && manifest.entities.length <= 100);
  const ids = new Set();
  for (const entity of manifest.entities) {
    assert.match(entity.id, /^Q[1-9]\d*$/);
    assert.ok(!ids.has(entity.id), "duplicate entity"); ids.add(entity.id);
    assert.ok(Number.isSafeInteger(entity.revision) && entity.revision > 0);
    assert.equal(entity.url, `https://www.wikidata.org/wiki/Special:EntityData/${entity.id}.json?revision=${entity.revision}`);
    assert.match(entity.sha256, /^[a-f0-9]{64}$/);
  }
  assert.ok(manifest.works.length > 0);
  const workIds = new Set();
  for (const work of manifest.works) {
    assert.ok(ids.has(work.id) && !workIds.has(work.id), "work must have one pinned entity");
    workIds.add(work.id);
    assert.ok(Object.hasOwn(types, work.type));
    assert.match(work.language, /^[a-z]{2,3}(-[A-Z]{2})?$/);
    assert.ok(typeof work.discipline === "string" && work.discipline.length > 0);
  }
}

// Canonical JSON avoids treating insignificant server key order as new data.
export function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === "object")
    return Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonical(value[key])]));
  return value;
}
export const digest = (entity) => createHash("sha256").update(JSON.stringify(canonical(entity))).digest("hex");
export function validateEntity(resource, entity) {
  assert.ok(entity?.id === resource.id && entity.lastrevid === resource.revision, `${resource.id}: revision mismatch`);
  assert.equal(digest(entity), resource.sha256, `${resource.id}: metadata digest mismatch`);
}
const values = (entity, property) => (entity.claims?.[property] ?? [])
  .filter((claim) => claim.rank !== "deprecated" && claim.mainsnak?.datavalue)
  .map((claim) => claim.mainsnak.datavalue.value);

function label(entity, language) {
  assert.ok(entity, "referenced author/journal entity is missing");
  const text = entity.labels?.[language]?.value ?? entity.labels?.mul?.value ?? entity.labels?.en?.value;
  assert.ok(typeof text === "string" && text.length > 0, "referenced entity has no usable label");
  return text;
}

function dateParts(value) {
  const match = /^\+(\d{4})-(\d{2})-(\d{2})T00:00:00Z$/.exec(value.time);
  assert.ok(match && value.precision >= 9 && value.precision <= 11, "unsupported bibliographic date");
  const parts = match.slice(1, value.precision - 7).map(Number);
  assert.ok(parts[0] > 0 && (parts.length < 2 || (parts[1] > 0 && parts[1] <= 12)));
  assert.ok(parts.length < 3 || (parts[2] > 0 && parts[2] <= 31));
  return parts;
}

function tex(text) {
  assert.ok(!String(text).includes("\\"), "literal backslashes are unsupported by the reader; see issue #90");
  const escapes = { "\\": "\\textbackslash{}", "{": "\\{", "}": "\\}", "%": "\\%", "&": "\\&", "#": "\\#", "_": "\\_", "$": "\\$", "~": "\\textasciitilde{}", "^": "\\textasciicircum{}" };
  return String(text).replace(/[\\{}%&#_$~^]/g, (c) => escapes[c]).replace(/\s+/gu, " ");
}

export function convert(manifest, entities) {
  const csl = [], absentDates = [];
  for (const work of manifest.works) {
    const entity = entities.get(work.id);
    assert.ok(entity, "work entity missing");
    const titles = values(entity, "P1476");
    const title = titles.find((v) => v.language === work.language)?.text ?? entity.labels?.[work.language]?.value;
    assert.ok(typeof title === "string" && title.length > 0, "work has no title in its recorded language");
    const authors = ["P50", "P2093"].flatMap((property) => (entity.claims?.[property] ?? [])
      .filter((claim) => claim.rank !== "deprecated" && claim.mainsnak?.datavalue)
      .map((claim) => ({
        order: Number(claim.qualifiers?.P1545?.[0]?.datavalue?.value ?? Number.MAX_SAFE_INTEGER),
        literal: property === "P50" ? label(entities.get(claim.mainsnak.datavalue.value.id), work.language) : claim.mainsnak.datavalue.value,
      })))
      .sort((a, b) => a.order - b.order).map(({ literal }) => ({ literal }));
    assert.ok(authors.length > 0 && authors.every((a) => typeof a.literal === "string" && a.literal.length > 0), "work has no usable authors");
    const item = { id: work.id, type: work.type, title, author: authors, language: work.language, URL: `https://www.wikidata.org/entity/${work.id}` };
    const dates = values(entity, "P577");
    if (dates.length) item.issued = { "date-parts": [dateParts(dates[0])] };
    else absentDates.push(work.id);
    const journal = values(entity, "P1433")[0];
    if (journal) item["container-title"] = label(entities.get(journal.id), work.language);
    for (const [property, field] of [["P356", "DOI"], ["P478", "volume"], ["P433", "issue"], ["P304", "page"]]) {
      const value = values(entity, property)[0];
      if (value !== undefined) { assert.equal(typeof value, "string"); item[field] = value; }
    }
    csl.push(item);
  }
  const bib = csl.map((item) => {
    const entries = [
      `title={{${tex(item.title)}}}`,
      `author={${item.author.map((a) => `{${tex(a.literal)}}`).join(" and ")}}`,
      `langid={${item.language}}`, `url={${tex(item.URL)}}`,
    ];
    if (item.issued) entries.push(`date={${item.issued["date-parts"][0].map((n, i) => String(n).padStart(i ? 2 : 4, "0")).join("-")}}`);
    for (const [field, name] of [["DOI", "doi"], ["container-title", "journal"], ["volume", "volume"], ["issue", "number"], ["page", "pages"]])
      if (item[field] !== undefined) entries.push(`${name}={${tex(item[field])}}`);
    return `@${types[item.type]}{${item.id},\n  ${entries.join(",\n  ")}\n}\n`;
  }).join("\n");
  return { csl, bib, absentDates };
}

export function verifyExport(expected, actual) {
  assert.equal(actual.length, expected.length, "reader skipped or duplicated a corpus record");
  const byId = new Map(actual.map((item) => [item.id, item]));
  assert.equal(byId.size, actual.length, "duplicate exported identity");
  for (const item of expected) {
    const exported = byId.get(item.id);
    assert.ok(exported, `reader skipped ${item.id}`);
    for (const field of fields) {
      let value = exported[field];
      if (field === "title" && typeof value === "string")
        value = value.replace(/<span class="nocase">|<\/span>/g, "").replace(/&lt;|&gt;|&amp;|&quot;|&#39;/g,
          (entity) => ({ "&lt;": "<", "&gt;": ">", "&amp;": "&", "&quot;": '"', "&#39;": "'" })[entity]);
      assert.deepEqual(value, item[field], `${item.id}: changed ${field}`);
    }
  }
}

async function download(resource) {
  const response = await fetch(resource.url, {
    headers: { "User-Agent": "IbisCorpusTests/0.1 (https://github.com/ibis-cite/ibis)" },
    signal: AbortSignal.timeout(30_000), redirect: "error",
  });
  assert.ok(response.ok, `${resource.id}: HTTP ${response.status}`);
  // Bound decompressed input, not just an untrusted Content-Length header.
  const chunks = []; let size = 0;
  for await (const chunk of response.body) {
    size += chunk.length;
    assert.ok(size <= 4 * 1024 * 1024, `${resource.id}: response exceeds 4 MiB`);
    chunks.push(chunk);
  }
  return JSON.parse(Buffer.concat(chunks).toString("utf8")).entities?.[resource.id];
}

async function main() {
  const [command, directory] = process.argv.slice(2);
  const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
  validateManifest(manifest);
  if (command === "check") { console.log(`CC0 manifest valid: ${manifest.works.length} works, ${manifest.entities.length} pinned entities`); return; }
  assert.ok(["fetch", "verify"].includes(command) && directory, "usage: node scripts/corpus.mjs check | fetch DIRECTORY | verify DIRECTORY");
  if (command === "verify") {
    verifyExport(JSON.parse(await readFile(join(directory, "expected.csl.json"), "utf8")), JSON.parse(await readFile(join(directory, "ibis.csl.json"), "utf8")));
    console.log("Corpus export preserves all projected fields"); return;
  }
  const cache = join(directory, "raw"); await mkdir(cache, { recursive: true });
  const entities = new Map();
  for (const resource of manifest.entities) {
    const path = join(cache, `${resource.id}-${resource.revision}.json`);
    let entity;
    try { entity = JSON.parse(await readFile(path, "utf8")); }
    catch (error) { if (error.code !== "ENOENT") throw error; entity = await download(resource); }
    validateEntity(resource, entity);
    entities.set(resource.id, entity);
    await writeFile(path, JSON.stringify(canonical(entity)) + "\n");
  }
  const { csl, bib, absentDates } = convert(manifest, entities);
  await writeFile(join(directory, "corpus.bib"), bib);
  await writeFile(join(directory, "expected.csl.json"), JSON.stringify(csl, null, 2) + "\n");
  await writeFile(join(directory, "provenance.json"), JSON.stringify({ manifest, absentDates }, null, 2) + "\n");
  console.log(`Generated ${csl.length} CC0 records; ${absentDates.length} have no publication date (recorded in provenance.json)`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => { console.error(`Corpus task failed: ${error.message}`); process.exitCode = 1; });
}
