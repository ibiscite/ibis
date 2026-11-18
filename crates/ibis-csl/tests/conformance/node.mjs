// citeproc-js's own column of the conformance run: the pinned processor under
// Node, without Ibis's adapter, reading locales from the same bundle by exact
// file name as citeproc-js's runner does. Reads the parsed fixtures as a JSON
// array on stdin and writes one { output } or { error } per fixture.
import fs from "node:fs";
import vm from "node:vm";
import { zstdDecompressSync } from "node:zlib";
import CSL from "../../vendor/citeproc.mjs";

const bundle = fs.readFileSync(new URL("../../vendor/csl-bundle.bin", import.meta.url));
const json = (frame) => JSON.parse(zstdDecompressSync(frame));
const blocksStart = 16 + bundle.readUInt32LE(8) + bundle.readUInt32LE(12);
const header = json(bundle.subarray(16, 16 + bundle.readUInt32LE(8)));
const locales = new Map(header.locales.map(([tag, , , block, offset, length]) => [tag, () => {
  const [at, size] = header.blocks[block];
  return zstdDecompressSync(bundle.subarray(blocksStart + at, blocksStart + at + size)).subarray(offset, offset + length).toString("utf8");
}]));
CSL.debug = () => {};
globalThis.CSL = CSL;
globalThis.__ibisLocaleXml = (tag) => locales.get(tag)?.() ?? null;
vm.runInThisContext(fs.readFileSync(new URL("driver.js", import.meta.url), "utf8"));
const fixtures = JSON.parse(fs.readFileSync(0, "utf8"));
const results = fixtures.map((fixture) => {
  try {
    return { output: __cslFixture(fixture) };
  } catch (error) {
    return { error: String(error?.message ?? error).slice(0, 200) };
  }
});
fs.writeFileSync(1, JSON.stringify(results));
