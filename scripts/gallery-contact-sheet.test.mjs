import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { frameFailure, renderFrames } from "./gallery-contact-sheet.mjs";

const cases = JSON.parse(await readFile(new URL("../fixtures/gallery-retries.json", import.meta.url), "utf8"));
const failure = (fixture, attempt) => frameFailure(fixture.name, fixture.stage,
  { name: fixture.error, message: "synthetic failure\nsecond line" }, fixture.messages, attempt);

for (const fixture of cases) {
  test(`${fixture.name} has a bounded, explicit retry policy`, async () => {
    let calls = 0;
    const result = await renderFrames([fixture], async (_, attempt) => ++calls === 1 ? failure(fixture, attempt) : null);
    assert.equal(calls, fixture.retryable ? 2 : 1);
    assert.equal(result.failed.length, fixture.retryable ? 0 : 1);
    assert.deepEqual(result.recovered, fixture.retryable ? [fixture.name] : []);
    assert.equal(result.failedAttempts.length, 1);
    assert.equal(result.failedAttempts[0].retryable, fixture.retryable);
    assert.equal(result.failedAttempts[0].screenshot, null, "unavailable capture remains explicit");
  });
}

test("a failed retry keeps both attempt diagnostics and never gets a third try", async () => {
  const fixture = cases.find((item) => item.retryable);
  let calls = 0;
  const result = await renderFrames([fixture], async (_, attempt) => { calls++; return failure(fixture, attempt); });
  assert.equal(calls, 2);
  assert.deepEqual(result.failedAttempts.map((item) => item.attempt), [1, 2]);
  assert.equal(result.failed.length, 1);
  assert.deepEqual(result.recovered, []);
});

test("a recovered frame retains its first-attempt screenshot and message", async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "ibis-gallery-retry-"));
  const file = path.join(directory, "failure-synthetic-attempt-1.png");
  try {
    const fixture = cases.find((item) => item.retryable);
    const result = await renderFrames([fixture], async (_, attempt) => {
      if (attempt === 2) return null;
      await writeFile(file, "synthetic failure capture");
      return { ...failure(fixture, attempt), screenshot: path.basename(file) };
    });
    assert.equal(await readFile(file, "utf8"), "synthetic failure capture");
    assert.equal(result.failedAttempts[0].message, "synthetic failure");
    assert.equal(result.failedAttempts[0].screenshot, path.basename(file));
    assert.deepEqual(result.recovered, [fixture.name]);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("workers capture every frame once and retry only the timed-out frame", async () => {
  const jobs = Array.from({ length: 9 }, (_, id) => ({ id }));
  const calls = [];
  const result = await renderFrames(jobs, async (job, attempt) => {
    calls.push([job.id, attempt]);
    if (job.id === 3 && attempt === 1) return { ...failure(cases.find((item) => item.retryable), attempt), frame: "frame3" };
    return null;
  });
  assert.deepEqual(calls.filter(([, attempt]) => attempt === 1).map(([id]) => id).sort(), jobs.map(({ id }) => id));
  assert.deepEqual(calls.filter(([, attempt]) => attempt === 2), [[3, 2]]);
  assert.deepEqual(result.failed, []);
  assert.deepEqual(result.recovered, ["frame3"]);
});
