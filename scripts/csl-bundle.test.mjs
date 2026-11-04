import assert from "node:assert/strict";
import test from "node:test";
import { zstdDecompressSync } from "node:zlib";
import { pack, sha256, styleMetadata } from "./csl-bundle.mjs";

const independent = (id, extra = "") => `<?xml version="1.0"?><style xmlns="http://purl.org/net/xbiblio/csl" class="in-text" default-locale="de-DE">
<info><title>Synthetic &amp; Style</title><id>http://www.zotero.org/styles/${id}</id><category citation-format="author-date"/><category field="biology"/><updated>2026-01-02T03:04:05+00:00</updated>${extra}</info></style>`;

test("a style's metadata comes from its own info block", () => {
  const style = styleMetadata("synthetic", independent("synthetic"));
  assert.deepEqual(style, { id: "synthetic", title: "Synthetic & Style", updated: "2026-01-02", parent: null, locale: "de-DE", format: "author-date", fields: ["biology"] });
  const dependent = styleMetadata("child", independent("child", `<link href="http://www.zotero.org/styles/parent" rel="independent-parent"/>`));
  assert.equal(dependent.parent, "parent");
});

test("a commented-out info block is not read", () => {
  const xml = independent("synthetic").replace("<info>", `<!-- <info><title>Old</title></info> --><info>`);
  assert.equal(styleMetadata("synthetic", xml).title, "Synthetic & Style");
});

test("a style that lies about its id or lacks a date is refused", () => {
  assert.throws(() => styleMetadata("other", independent("synthetic")), /declares id/);
  assert.throws(() => styleMetadata("synthetic", independent("synthetic").replace(/<updated>.*<\/updated>/, "")), /updated date/);
  assert.throws(() => styleMetadata("synthetic", independent("synthetic").replace("Synthetic", "Syn\u0007thetic")), /control character/);
});

const file = (id, parent = null) => ({ id, title: id, updated: "2026-01-02", parent, locale: null, format: "numeric", fields: [], bytes: Buffer.from(independent(id)) });
const options = (styles) => ({ styles, locales: [{ tag: "en-US", native: "English", english: "English", bytes: Buffer.from("<locale/>") }], renames: {}, curated: [], releases: {}, primary: {} });

test("the bundle reads back through its index", () => {
  const bundle = pack(options([file("beta"), file("alpha"), file("child", "alpha")]));
  assert.equal(bundle.subarray(0, 8).toString(), "IBISCSL1");
  const header = bundle.readUInt32LE(8);
  const rows = bundle.readUInt32LE(12);
  const index = JSON.parse(zstdDecompressSync(bundle.subarray(16, 16 + header)));
  const styles = JSON.parse(zstdDecompressSync(bundle.subarray(16 + header, 16 + header + rows)));
  assert.deepEqual(styles.map((row) => row[0]), ["alpha", "beta", "child"]);
  const start = 16 + header + rows;
  const [at, length] = index.blocks[styles[1][7]];
  const block = zstdDecompressSync(bundle.subarray(start + at, start + at + length));
  const read = block.subarray(styles[1][8], styles[1][8] + styles[1][9]);
  assert.equal(sha256(read), sha256(Buffer.from(independent("beta"))));
});

test("a broken reference in the release stops the build", () => {
  assert.throws(() => pack(options([file("child", "missing")])), /parent missing is missing/);
  assert.throws(() => pack(options([file("a", "b"), file("b", "c"), file("c")])), /itself dependent/);
  assert.throws(() => pack({ ...options([file("a")]), renames: { old: "gone" } }), /rename target/);
  assert.throws(() => pack({ ...options([file("a")]), curated: [{ id: "gone", title: "Gone", name: "Gone" }] }), /curated style/);
  assert.throws(() => pack({ ...options([file("a")]), curated: [{ id: "a", title: "A" }] }), /needs a short name/);
  assert.throws(() => pack({ ...options([file("a")]), curated: [{ id: "a", title: "A", name: "A".repeat(25) }] }), /needs a short name/);
});
