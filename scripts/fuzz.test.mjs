import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";
import { mkdtempSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { failure, minimize, report, seedCorpus } from "./fuzz.mjs";

test("failure signatures ignore addresses and paths, distinguish frames and resource failures", () => {
  const log = (address, frame) => `ERROR: libFuzzer: deadly signal\n #0 ${address} in ${frame} /build/path/file.rs:10:2`;
  assert.equal(failure("csl_export", log("0xabc", "ibis_core::bibtex::load_str")).signature,
    failure("csl_export", log("0xdef", "ibis_core::bibtex::load_str")).signature);
  assert.notEqual(failure("csl_export", log("0xabc", "ibis_core::bibtex::load_str")).signature,
    failure("csl_export", log("0xabc", "ibis_core::bibtex::parse_entry")).signature);
  assert.equal(failure("csl_export", "ERROR: libFuzzer: timeout after 5 seconds").kind, "timeout");
  assert.equal(failure("csl_export", "ERROR: libFuzzer: out-of-memory (malloc(123))").kind, "out-of-memory");
  assert.equal(failure("csl_export", "cargo: compilation failed"), null);
});

for (const kind of ["crash", "timeout", "out-of-memory"]) {
  test(`minimization retains the same ${kind} and rejects unrelated failures`, () => {
    const input = Buffer.from("prefix!suffix");
    const result = minimize(input, (candidate) => candidate.includes(33) ? kind : "different", kind);
    assert.deepEqual(result, Buffer.from("!"));
  });
}

test("report searches every open-issue page before creating a duplicate", async () => {
  let calls = 0;
  const problem = failure("csl_export", "ERROR: libFuzzer: timeout after 5 seconds");
  const request = async (path, body) => {
    calls++;
    assert.equal(body, undefined);
    return path.endsWith("&page=1") ? Array(100).fill({ body: "unrelated" }) : [{ body: `<!-- fuzz:${problem.signature} -->` }];
  };
  assert.equal(await report("owner/repo", "csl_export", Buffer.from("x"), problem, "https://example.org/run", request), false);
  assert.equal(calls, 2);
});

test("report contains lossless input, failure and reproduction details, escaping hostile markdown", async () => {
  let created;
  const input = Buffer.from([96, 96, 96, 255, 0]);
  const problem = failure("split_parse", "ERROR: libFuzzer: deadly signal");
  await report("owner/repo", "split_parse", input, problem, "https://example.org/run", async (_path, body) => {
    if (!body) return [];
    created = body;
  });
  assert.deepEqual(created.labels, ["type:bug", "area:infra"]);
  assert.match(created.title, /^Fuzzing found a crash in split_parse \([0-9a-f]{12}\)$/);
  assert.ok(created.body.includes(input.toString("base64")));
  assert.ok(created.body.includes("cargo +nightly fuzz run split_parse"));
  assert.ok(created.body.includes("deadly signal"));
  assert.ok(!created.body.includes("```�"));
});

test("CSL corpus keeps colliding legacy inputs, deduplicates bytes and seeds ordinary targets", () => {
  const previous = process.cwd();
  const root = mkdtempSync(join(tmpdir(), "ibis-fuzz-corpus-"));
  try {
    process.chdir(root);
    for (const dir of ["fixtures/bibtex", "fuzz/corpus/csl_export", "fuzz/corpus/load_str", "fuzz/seeds/csl_export"]) mkdirSync(dir, { recursive: true });
    writeFileSync("fixtures/bibtex/fixture.bib", "fixture");
    writeFileSync("fuzz/seeds/csl_export/synthetic.bib", "target seed");
    writeFileSync("fuzz/corpus/csl_export/collision", "current");
    writeFileSync("fuzz/corpus/load_str/collision", Buffer.from([0, 255]));
    writeFileSync("fuzz/corpus/load_str/empty", "");
    const occupied = createHash("sha256").update(Buffer.from([0, 255])).digest("hex");
    writeFileSync(`fuzz/corpus/csl_export/${occupied}`, "distinct current bytes");
    seedCorpus("csl_export");
    assert.equal(readFileSync(`fuzz/corpus/csl_export/${occupied}`, "utf8"), "distinct current bytes");
    const files = readdirSync("fuzz/corpus/csl_export");
    assert.equal(files.length, 6);
    assert.ok(files.some((file) => readFileSync(`fuzz/corpus/csl_export/${file}`).equals(Buffer.from([0, 255]))));
    assert.equal(readFileSync("fuzz/corpus/csl_export/collision", "utf8"), "current");
    seedCorpus("csl_export");
    assert.deepEqual(readdirSync("fuzz/corpus/csl_export"), files);
    seedCorpus("split_parse");
    assert.deepEqual(readdirSync("fuzz/corpus/split_parse"), ["seed-fixture.bib"]);
  } finally {
    process.chdir(previous);
    rmSync(root, { recursive: true, force: true });
  }
});
