import assert from "node:assert/strict";
import test from "node:test";
import { failure, minimize, report } from "./fuzz.mjs";

test("failure signatures ignore addresses and paths, distinguish frames and resource failures", () => {
  const log = (address, frame) => `ERROR: libFuzzer: deadly signal\n #0 ${address} in ${frame} /build/path/file.rs:10:2`;
  assert.equal(failure("load_str", log("0xabc", "ibis_core::bibtex::load_str")).signature,
    failure("load_str", log("0xdef", "ibis_core::bibtex::load_str")).signature);
  assert.notEqual(failure("load_str", log("0xabc", "ibis_core::bibtex::load_str")).signature,
    failure("load_str", log("0xabc", "ibis_core::bibtex::parse_entry")).signature);
  assert.equal(failure("load_str", "ERROR: libFuzzer: timeout after 5 seconds").kind, "timeout");
  assert.equal(failure("load_str", "ERROR: libFuzzer: out-of-memory (malloc(123))").kind, "out-of-memory");
  assert.equal(failure("load_str", "cargo: compilation failed"), null);
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
  const problem = failure("load_str", "ERROR: libFuzzer: timeout after 5 seconds");
  const request = async (path, body) => {
    calls++;
    assert.equal(body, undefined);
    return path.endsWith("&page=1") ? Array(100).fill({ body: "unrelated" }) : [{ body: `<!-- fuzz:${problem.signature} -->` }];
  };
  assert.equal(await report("owner/repo", "load_str", Buffer.from("x"), problem, "https://example.org/run", request), false);
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
