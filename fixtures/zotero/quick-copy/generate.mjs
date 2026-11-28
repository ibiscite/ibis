// Zotero's own Quick Copy output for every invented item of the mapping corpus.
// Run from any directory with Node 22+ after `pnpm -C apps/desktop install`
// (for jsdom, which stands in for the browser DOM Zotero's code expects).
// Runs pinned, unmodified Zotero code (quickCopy.js, style.js, cite.js) over the
// citeproc-js, styles and locale Ibis bundles. Never runs Ibis's mapper or
// formatter, Zotero itself, or a real library.
import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { zstdDecompressSync } from "node:zlib";
import path from "node:path";
import vm from "node:vm";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../../..");
const vendor = path.join(root, "crates/ibis-sources/vendor/zotero");
const manifest = JSON.parse(await readFile(path.join(vendor, "manifest.json"), "utf8"));
process.env.TZ = manifest.oracleProfile.timeZone;
for (const { out, sha256 } of manifest.files) {
  const bytes = await readFile(path.join(vendor, out));
  if (createHash("sha256").update(bytes).digest("hex") !== sha256) throw new Error(`Pinned oracle digest changed: ${out}`);
}
const { JSDOM } = createRequire(path.join(root, "apps/desktop/package.json"))("jsdom");
const { window } = new JSDOM("");
const { default: CSL } = await import(path.join(root, "crates/ibis-csl/vendor/citeproc.mjs"));

// The styles and locale Ibis bundles, byte for byte (crates/ibis-csl/vendor).
const bundle = await readFile(path.join(root, "crates/ibis-csl/vendor/csl-bundle.bin"));
const frame = (bytes) => JSON.parse(zstdDecompressSync(bytes));
const blocksStart = 16 + bundle.readUInt32LE(8) + bundle.readUInt32LE(12);
const header = frame(bundle.subarray(16, 16 + bundle.readUInt32LE(8)));
const rows = frame(bundle.subarray(16 + bundle.readUInt32LE(8), blocksStart));
const unpack = (block, offset, length) => {
  const [at, size] = header.blocks[block];
  return zstdDecompressSync(bundle.subarray(blocksStart + at, blocksStart + at + size)).subarray(offset, offset + length).toString("utf8");
};
const curated = JSON.parse(await readFile(path.join(root, "crates/ibis-csl/vendor/manifest.json"), "utf8")).curated.map(({ id }) => id);
const locale = manifest.oracleProfile.locale;
// Every locale, as Zotero ships them: a style's own default-locale wins (GB/T is zh-CN).
const locales = new Map(header.locales.map((row) => [`chrome://zotero/content/locale/csl/locales-${row[0]}.xml`, row]));

// Stand-ins for the parts of Zotero these three files call. Zotero.Utilities'
// xpath helpers use the DOM's own XPath; everything else answers one fixed value.
const styles = new Map();
const items = new Map();
const zotero = {
  debug() {},
  logError(error) { throw error; },
  locale,
  Prefs: {
    get(key) {
      if (key === "export.quickCopy.dragLimit") return 50;
      if (key === "export.quickCopy.locale") return locale;
      throw new Error(`Unexpected preference: ${key}`);
    },
  },
  File: {
    getContents: (id) => sources.get(id),
    getResource(url) {
      const row = locales.get(url);
      if (!row) throw new Error(`Unexpected resource: ${url}`);
      return unpack(row[3], row[4], row[5]);
    },
  },
  // COinS metadata spans are invisible; the harness has no OpenURL, so none are added.
  OpenURL: { createContextObject() { return false; } },
  Items: { get: (id) => items.get(String(id)) },
  Utilities: {
    // HTML only (Zotero.Cite.makeFormattedBibliography); jsdom's XPath cannot read
    // namespaced XML, so styles are read below instead of by Zotero.Style.
    xpath(node, expression) {
      const doc = node.ownerDocument ?? node;
      const result = doc.evaluate(expression, node, null, window.XPathResult.ORDERED_NODE_ITERATOR_TYPE, null);
      const nodes = [];
      for (let found; (found = result.iterateNext());) nodes.push(found);
      return nodes;
    },
  },
  CiteProc: { CSL },
};
// Zotero.Styles registers a memory-pressure observer when it loads; nothing calls it here.
const firefox = { ChromeUtils: { generateQI: () => null }, Services: { obs: { addObserver() {} } } };
const context = vm.createContext({ Zotero: zotero, ...firefox, DOMParser: window.DOMParser, XMLSerializer: window.XMLSerializer, console });
for (const file of ["cite.js", "style.js", "quickCopy.js"]) {
  vm.runInContext(await readFile(path.join(vendor, file), "utf8"), context, { filename: file });
}
// Zotero.Styles keeps its own code; only the lookup of installed styles is the harness's.
zotero.Styles.get = (id) => styles.get(id);
// Each item is the CSL that Zotero's own retrieveItem gave for it in the mapping
// corpus (fixtures/zotero/mapping), which records exactly that output minus `id`.
zotero.Cite.System.prototype.retrieveItem = (id) => ({ ...items.get(String(id)).csl, id });

const cases = JSON.parse(await readFile(path.join(root, "fixtures/zotero/mapping/cases.json"), "utf8"));
// What Zotero.Style's constructor reads from a style file, for an installed
// style at `path` (its short id), read here with the DOM's own selectors.
const shortId = (styleID) => styleID.replace(/^.*\//, "");
const sources = new Map();
function install(id) {
  const row = rows.find((candidate) => candidate[0] === id);
  const xml = unpack(row[7], row[8], row[9]);
  const doc = new window.DOMParser().parseFromString(xml, "application/xml");
  const parent = doc.querySelector('info > link[rel="independent-parent"], info > link[rel="source"]')?.getAttribute("href") ?? null;
  const style = Object.assign(Object.create(zotero.Style.prototype), {
    type: "csl", path: id, styleID: doc.querySelector("info > id").textContent,
    locale: doc.documentElement.getAttribute("default-locale") || null, source: parent,
    _class: doc.documentElement.getAttribute("class"), _version: doc.documentElement.getAttribute("version"),
    _cachedEngines: new Map(),
  });
  sources.set(id, xml);
  styles.set(style.styleID, style);
  if (parent && !styles.has(parent)) install(shortId(parent));
  return style;
}
const curatedStyles = curated.map(install);
const results = [];
for (const [index, { name, expected }] of cases.entries()) {
  const item = { id: index + 1, csl: expected, isNote: () => false };
  items.set(String(item.id), item);
  for (const [at, { styleID }] of curatedStyles.entries()) {
    const id = curated[at];
    const format = { mode: "bibliography", contentType: "", id: styleID, locale };
    try {
      // Shift-copy gives citations; a plain copy gives the bibliography.
      const citation = zotero.QuickCopy.getContentFromItems([item], format, null, true);
      const bibliography = zotero.QuickCopy.getContentFromItems([item], format, null, false);
      results.push({ name, style: id, citation, bibliography });
    } catch (error) {
      // Zotero shows an error and copies nothing.
      results.push({ name, style: id, error: String(error.message ?? error) });
    }
  }
}
// One case a line: reviewable diffs without doubling the file's size.
const formats = Object.fromEntries(curated.map((id) => {
  const doc = new window.DOMParser().parseFromString(sources.get(id), "application/xml");
  return [id, doc.querySelector("info > category[citation-format]").getAttribute("citation-format")];
}));
const head = JSON.stringify({ zotero: manifest.sources.zotero.commit, locale, formats });
const output = `${head.slice(0, -1)},"cases":[\n${results.map((result) => JSON.stringify(result)).join(",\n")}\n]}\n`;
const outputPath = path.join(here, "expected.json");
if (process.argv.includes("--check")) {
  if (await readFile(outputPath, "utf8") !== output) throw new Error("Zotero Quick Copy fixture drift");
} else {
  await writeFile(outputPath, output);
}
console.log(`${cases.length} invented items; ${curated.length} styles; ${results.length} Quick Copy cases`);
