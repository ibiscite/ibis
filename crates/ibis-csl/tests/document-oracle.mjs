// Independent direct calls to the pinned processor: no production bridge.
import fs from "node:fs";
import { zstdDecompressSync } from "node:zlib";
import CSL from "../vendor/citeproc.mjs";
const bundle = fs.readFileSync(new URL("../vendor/csl-bundle.bin", import.meta.url));
const headerEnd = 16 + bundle.readUInt32LE(8), blocksStart = headerEnd + bundle.readUInt32LE(12);
const header = JSON.parse(zstdDecompressSync(bundle.subarray(16, headerEnd)));
const rows = JSON.parse(zstdDecompressSync(bundle.subarray(headerEnd, blocksStart)));
const unpack = (block, offset, length) => {
  const [at, size] = header.blocks[block];
  return zstdDecompressSync(bundle.subarray(blocksStart + at, blocksStart + at + size)).subarray(offset, offset + length).toString("utf8");
};
const cases = JSON.parse(fs.readFileSync(new URL("document-cases.json", import.meta.url)));
const results = cases.documents.map((request) => {
  const row = rows.find((row) => row[0] === request.style);
  const items = new Map(structuredClone(cases.items).map((item) => [item.id, item]));
  const engine = new CSL.Engine({ retrieveItem: (id) => items.get(id), retrieveLocale: (tag) => {
    const locale = header.locales.find((row) => row[0] === tag);
    return locale && unpack(locale[3], locale[4], locale[5]);
  }}, unpack(row[7], row[8], row[9]), request.locale, true);
  const citations = request.clusters.map((cluster, index) => ({ citationID: `oracle-${index}`,
    citationItems: cluster.ids.map((id) => ({ id })), properties: { noteIndex: cluster.note_index } }));
  // A separate document API from Ibis's incremental update loop; both must
  // produce the final sequence, including retroactive year suffix changes.
  const clusters = engine.rebuildProcessorState(citations, "text", []).map((entry) => entry[2]);
  engine.setOutputFormat("text");
  const bibliography = engine.makeBibliography();
  if (bibliography && bibliography[0].bibliography_errors.length) throw Error("oracle bibliography refused");
  return { clusters, bibliography: bibliography ? bibliography[1].map((text) => text.replace(/\s*\n\s*/g, " ").trim()) : [] };
});
process.stdout.write(JSON.stringify(results));
