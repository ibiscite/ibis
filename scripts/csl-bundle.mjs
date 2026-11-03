#!/usr/bin/env node
// Build crates/ibis-csl/vendor/csl-bundle.bin and csl-bundle.json from the tagged
// CSL style and locale releases pinned in vendor/manifest.json. Run by hand or by
// the styles workflow; Ibis never downloads styles at run time (decision 0007).
//
//   node scripts/csl-bundle.mjs           rebuild at the pins and check the bytes
//   node scripts/csl-bundle.mjs --update  move the pins to the newest tags
//
// The bundle is `IBISCSL1`, the byte lengths of the two index frames (u32 LE),
// then zstd frames: a small header, the style rows, and 1 MiB blocks of
// concatenated files. Reading one style therefore unpacks about 1 MiB.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { lstatSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { constants, zstdCompressSync } from "node:zlib";

const BLOCK_BYTES = 1024 * 1024;
const MAX_FILE_BYTES = 1024 * 1024;
const MAX_ARCHIVE_BYTES = 256 * 1024 * 1024;
const ENTITIES = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'" };

export const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

function decode(text) {
  return text.replace(/&(?:(amp|lt|gt|quot|apos)|#(\d{1,7})|#x([0-9a-fA-F]{1,6}));/g, (_, name, decimal, hex) =>
    name ? ENTITIES[name] : String.fromCodePoint(decimal ? Number(decimal) : parseInt(hex, 16)));
}

function attribute(tag, name) {
  const found = new RegExp(`\\s${name}\\s*=\\s*(?:"([^"]*)"|'([^']*)')`).exec(tag);
  return found ? decode(found[1] ?? found[2]) : null;
}

/** What a style says about itself, read from its `<style>` and `<info>` only. */
export function styleMetadata(id, xml) {
  const text = xml.replace(/<!--[\s\S]*?-->/g, "");
  const style = /<style\b[^>]*>/.exec(text)?.[0];
  const info = /<info>([\s\S]*?)<\/info>/.exec(text)?.[1];
  if (!style || !info) throw new Error(`${id}: no <style> or <info>`);
  const element = (name) => {
    const found = new RegExp(`<${name}(?:\\s[^>]*)?>([^<]*)</${name}>`).exec(info)?.[1];
    return found === undefined ? null : decode(found).replace(/\s+/g, " ").trim();
  };
  const title = element("title");
  const updated = element("updated");
  const declared = element("id");
  if (!title || !updated) throw new Error(`${id}: a style needs a title and an updated date`);
  if (declared && !declared.endsWith(`/${id}`) && declared !== id) throw new Error(`${id}: declares id ${declared}`);
  // Joiners stay: Persian and Indic titles need them. Overrides could reorder a menu.
  if (/[\p{Cc}\u202a-\u202e\u2066-\u206f]/u.test(title + updated)) throw new Error(`${id}: control character in title`);
  const links = [...info.matchAll(/<link\b[^>]*>/g)].map(([tag]) => tag);
  const parentLink = links.find((tag) => attribute(tag, "rel") === "independent-parent");
  const parent = parentLink ? attribute(parentLink, "href")?.split("/").pop() ?? null : null;
  const categories = [...info.matchAll(/<category\b[^>]*>/g)].map(([tag]) => tag);
  const format = categories.map((tag) => attribute(tag, "citation-format")).find(Boolean) ?? null;
  const fields = categories.map((tag) => attribute(tag, "field")).filter(Boolean);
  const plain = /^[A-Za-z0-9_-]*$/;
  const locale = attribute(style, "default-locale");
  if (![format ?? "", locale ?? "", ...fields].every((value) => plain.test(value))) throw new Error(`${id}: unexpected characters in a category or locale`);
  return { id, title, updated: updated.slice(0, 10), parent, locale, format, fields };
}

const compress = (bytes) => zstdCompressSync(bytes, {
  pledgedSrcSize: bytes.length,
  params: {
    [constants.ZSTD_c_compressionLevel]: 19,
    [constants.ZSTD_c_checksumFlag]: 1,
  },
});

/** Packs files into blocks, keeping each file whole and adjacent files together. */
function blocks(files, output) {
  let pending = [];
  let size = 0;
  const flush = () => {
    if (!pending.length) return;
    output.push(Buffer.concat(pending));
    pending = [];
    size = 0;
  };
  for (const file of files) {
    file.block = output.length;
    file.offset = size;
    pending.push(file.bytes);
    size += file.bytes.length;
    if (size >= BLOCK_BYTES) flush();
  }
  flush();
}

/** Lays out the complete bundle. Pure, so the format has a test without a download. */
export function pack({ styles, locales, renames, curated, releases, primary }) {
  const byId = new Map(styles.map((style) => [style.id, style]));
  for (const style of styles) {
    if (style.parent && !byId.get(style.parent)) throw new Error(`${style.id}: parent ${style.parent} is missing`);
    if (style.parent && byId.get(style.parent).parent) throw new Error(`${style.id}: its parent is itself dependent`);
  }
  for (const id of Object.values(renames)) if (!byId.has(id)) throw new Error(`rename target ${id} is missing`);
  for (const { id } of curated) if (!byId.has(id)) throw new Error(`curated style ${id} is missing`);
  // Independent styles share a little; dependent ones are near copies of each other.
  const ordered = [...styles].sort((a, b) => Number(Boolean(a.parent)) - Number(Boolean(b.parent)) || (a.id < b.id ? -1 : 1));
  const raw = [];
  blocks(ordered, raw);
  blocks(locales, raw);
  const frames = raw.map((bytes) => compress(bytes));
  const table = [];
  let at = 0;
  for (const [index, frame] of frames.entries()) {
    table.push([at, frame.length, raw[index].length]);
    at += frame.length;
  }
  const header = compress(Buffer.from(JSON.stringify({
    releases,
    blocks: table,
    locales: locales.map(({ tag, native, english, block, offset, bytes }) => [tag, native, english, block, offset, bytes.length]),
    primary,
    renames,
    curated: curated.map(({ id, title }) => [id, title]),
  })));
  const rows = compress(Buffer.from(JSON.stringify([...styles].sort((a, b) => (a.id < b.id ? -1 : 1)).map((style) => [
    style.id, style.title, style.updated, style.parent, style.locale, style.format, style.fields,
    style.block, style.offset, style.bytes.length,
  ]))));
  const lengths = Buffer.alloc(8);
  lengths.writeUInt32LE(header.length, 0);
  lengths.writeUInt32LE(rows.length, 4);
  return Buffer.concat([Buffer.from("IBISCSL1"), lengths, header, rows, ...frames]);
}

function run(command, args) {
  const result = spawnSync(command, args, { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
  if (result.status !== 0) throw new Error(`${command} failed: ${result.stderr?.trim().slice(0, 500)}`);
  return result.stdout.trim();
}

/** The tag whose commit is the release to pin, newest version first. */
function newestTag(repo) {
  const lines = run("git", ["ls-remote", "--tags", "--sort=-v:refname", `https://github.com/${repo}.git`, "v*"]).split("\n");
  const [commit, ref] = lines.find((line) => !line.endsWith("^{}")).split(/\s+/);
  return { tag: ref.replace("refs/tags/", ""), commit: lines.find((line) => line.endsWith(`${ref}^{}`))?.split(/\s+/)[0] ?? commit };
}

async function download(source, into) {
  const url = `https://codeload.github.com/${source.repo}/tar.gz/${source.commit}`;
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
  const bytes = Buffer.from(await response.arrayBuffer());
  if (bytes.length > MAX_ARCHIVE_BYTES) throw new Error(`${url}: larger than ${MAX_ARCHIVE_BYTES} bytes`);
  const archive = join(into, "archive.tar.gz");
  writeFileSync(archive, bytes);
  const out = join(into, "tree");
  run("mkdir", [out]);
  run("tar", ["xzf", archive, "-C", out, "--strip-components=1"]);
  return out;
}

/** A regular file from the archive: a link could point outside it. */
function checkedFile(path) {
  if (!lstatSync(path).isFile()) throw new Error(`${path}: not a regular file`);
  const bytes = readFileSync(path);
  if (bytes.length > MAX_FILE_BYTES) throw new Error(`${path}: larger than ${MAX_FILE_BYTES} bytes`);
  return bytes;
}

export function readStyles(tree) {
  const styles = [];
  const directories = [[tree, false], [join(tree, "dependent"), true]];
  if (!lstatSync(join(tree, "dependent")).isDirectory()) throw new Error("dependent: not a directory");
  for (const [directory, dependent] of directories) {
    for (const name of readdirSync(directory).sort()) {
      if (!name.endsWith(".csl")) continue;
      const id = name.slice(0, -4);
      const bytes = checkedFile(join(directory, name));
      const style = { ...styleMetadata(id, bytes.toString("utf8")), bytes, path: `styles/${dependent ? "dependent/" : ""}${name}` };
      if (Boolean(style.parent) !== dependent) throw new Error(`${id}: parent link does not match its folder`);
      if (styles.some((other) => other.id === id)) throw new Error(`${id}: repeated`);
      styles.push(style);
    }
  }
  return styles;
}

export function readLocales(tree) {
  const catalogue = JSON.parse(checkedFile(join(tree, "locales.json")).toString("utf8"));
  for (const names of Object.values(catalogue["language-names"])) {
    if (names.some((name) => /[\p{Cc}\u202a-\u202e\u2066-\u206f]/u.test(name))) throw new Error("control character in a language name");
  }
  const locales = readdirSync(tree).sort().flatMap((name) => {
    const tag = /^locales-(.+)\.xml$/.exec(name)?.[1];
    if (!tag) return [];
    const [native, english] = catalogue["language-names"][tag] ?? [tag, tag];
    return [{ tag, native, english, bytes: checkedFile(join(tree, name)), path: `locales/${name}` }];
  });
  return { locales, primary: catalogue["primary-dialects"] };
}

async function main() {
  const root = join(dirname(fileURLToPath(import.meta.url)), "..");
  const vendor = join(root, "crates/ibis-csl/vendor");
  const manifestPath = join(vendor, "manifest.json");
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  const update = process.argv.includes("--update");
  if (update) {
    for (const name of ["styles", "locales"]) Object.assign(manifest.sources[name], newestTag(manifest.sources[name].repo));
  }
  const work = mkdtempSync(join(tmpdir(), "ibis-csl-bundle-"));
  try {
    run("mkdir", [join(work, "styles"), join(work, "locales")]);
    const stylesTree = await download(manifest.sources.styles, join(work, "styles"));
    const localesTree = await download(manifest.sources.locales, join(work, "locales"));
    const styles = readStyles(stylesTree);
    const { locales, primary } = readLocales(localesTree);
    const renames = JSON.parse(checkedFile(join(stylesTree, "renamed-styles.json")).toString("utf8"));
    const releases = Object.fromEntries(["styles", "locales"].map((name) => {
      const { repo, tag, commit } = manifest.sources[name];
      return [name, { repo, tag, commit }];
    }));
    const bundle = pack({ styles, locales, renames, curated: manifest.curated, releases, primary });
    const record = {
      releases,
      bundle: { path: "csl-bundle.bin", bytes: bundle.length, sha256: sha256(bundle) },
      renames,
      primary,
      files: Object.fromEntries([...styles, ...locales].map(({ path, bytes }) => [path, sha256(bytes)]).sort(([a], [b]) => (a < b ? -1 : 1))),
    };
    const recordPath = join(vendor, "csl-bundle.json");
    if (!update) {
      const pinned = JSON.parse(readFileSync(recordPath, "utf8"));
      for (const part of ["files", "renames", "primary"]) {
        // A manifest older than a part's record has nothing to compare it with.
        if (part in pinned && JSON.stringify(pinned[part]) !== JSON.stringify(record[part])) throw new Error(`the pinned release's ${part} no longer match csl-bundle.json`);
      }
    }
    writeFileSync(join(vendor, "csl-bundle.bin"), bundle);
    writeFileSync(recordPath, `${JSON.stringify(record, null, 1)}\n`);
    if (update) writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
    console.log(`${styles.length} styles, ${locales.length} locales, ${bundle.length} bytes`);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
