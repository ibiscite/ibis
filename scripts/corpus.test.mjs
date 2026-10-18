import assert from "node:assert/strict";
import test from "node:test";
import { convert, digest, validateEntity, validateManifest, verifyExport } from "./corpus.mjs";

const value = (value, qualifiers) => ({ mainsnak: { datavalue: { value } }, qualifiers });
const manifest = {
  schema_version: 1,
  license: "CC0-1.0",
  license_url: "https://www.wikidata.org/wiki/Wikidata:Licensing",
  retrieved: "2026-10-17",
  entities: ["Q1", "Q2"].map((id) => ({
    id, revision: 123, sha256: "a".repeat(64),
    url: `https://www.wikidata.org/wiki/Special:EntityData/${id}.json?revision=123`,
  })),
  works: [{ id: "Q1", language: "ja", type: "book", discipline: "literature" }],
};
const entities = new Map([
  ["Q1", { claims: {
    P1476: [value({ text: "記憶 & {A} %_$#~^ <B>", language: "ja" })],
    P50: [value({ id: "Q2" })],
    P577: [value({ time: "+2024-02-03T00:00:00Z", precision: 10 })],
  } }],
  ["Q2", { labels: { ja: { value: "例 著者" } } }],
]);

test("manifest requires recorded CC0 provenance, immutable URLs, dates and digests", () => {
  assert.doesNotThrow(() => validateManifest(manifest));
  for (const patch of [
    { license: "unknown" }, { retrieved: "2026-02-30" }, { retrieved: "" }, { retrieved: "9999-01-01" },
    { entities: [{ ...manifest.entities[0], url: "https://example.org/data" }] },
    { entities: [{ ...manifest.entities[0], sha256: "invalid" }] },
    { entities: [manifest.entities[0], manifest.entities[0]] },
  ]) assert.throws(() => validateManifest({ ...manifest, ...patch }));
});

test("conversion preserves native script, author identity, literal syntax and date precision", () => {
  const result = convert(manifest, entities);
  assert.equal(result.csl[0].title, entities.get("Q1").claims.P1476[0].mainsnak.datavalue.value.text);
  assert.deepEqual(result.csl[0].author, [{ literal: "例 著者" }]);
  assert.deepEqual(result.csl[0].issued, { "date-parts": [[2024, 2]] });
  for (const escaped of ["\\&", "\\{", "\\}", "\\%", "\\_", "\\$", "\\#"])
    assert.ok(result.bib.includes(escaped));
  assert.ok(result.bib.includes("langid={ja}"));
});

test("unavailable titles, author entities and unsupported dates are explicit failures", () => {
  const copy = () => new Map(structuredClone([...entities]));
  const noTitle = copy(); noTitle.get("Q1").claims.P1476 = [];
  const noAuthor = copy(); noAuthor.delete("Q2");
  const badDate = copy(); badDate.get("Q1").claims.P577[0].mainsnak.datavalue.value.time = "bad";
  for (const input of [noTitle, noAuthor, badDate]) assert.throws(() => convert(manifest, input));
});

test("literal source backslashes fail visibly until reader issue #90 is fixed", () => {
  const input = new Map(structuredClone([...entities]));
  input.get("Q1").claims.P1476[0].mainsnak.datavalue.value.text = "A \\ B";
  assert.throws(() => convert(manifest, input), /literal backslashes are unsupported/);
});

test("downloaded and cached entities must match pinned identity, revision and digest", () => {
  const entity = { id: "Q1", lastrevid: 123, labels: { ja: { value: "記憶" } } };
  const resource = { id: "Q1", revision: 123, sha256: digest(entity) };
  assert.doesNotThrow(() => validateEntity(resource, entity));
  assert.equal(digest({ lastrevid: 123, labels: entity.labels, id: "Q1" }), resource.sha256);
  for (const patch of [{ id: "Q2" }, { lastrevid: 124 }, { labels: {} }])
    assert.throws(() => validateEntity(resource, { ...entity, ...patch }));
});

test("export verification rejects skipped records and changed bibliographic fields", () => {
  const expected = convert(manifest, entities).csl;
  assert.doesNotThrow(() => verifyExport(expected, structuredClone(expected)));
  assert.throws(() => verifyExport(expected, []));
  for (const field of ["title", "author", "issued", "language", "type"]) {
    const changed = structuredClone(expected); delete changed[0][field];
    assert.throws(() => verifyExport(expected, changed));
  }
});
