import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { classify } from "./ci-paths.mjs";
import { recordCounts } from "./release-counts.mjs";

const header = "date,asset_name,download_count,updated_at\n";
const sha = (n) => n.toString(16).padStart(40, "0");
const monday = "2026-10-26T01:15:00Z";
const nextMonday = "2026-11-02T01:15:00Z";
const packageUpdated = "2026-10-25T20:00:00Z";
const manifestUpdated = "2026-10-25T20:01:00Z";

// In-memory Git objects and atomic ref updates; no network or local Git.
function github({ csv = null } = {}) {
  const state = {
    head: csv === null ? null : sha(1),
    csv,
    release: { id: 12, tag_name: "stable", draft: false },
    assets: [
      { id: 20, name: "Ibis-synthetic.dmg", download_count: 40, updated_at: packageUpdated },
      { id: 21, name: "Ibis-synthetic.app.tar.gz", download_count: 9, updated_at: packageUpdated },
      { id: 22, name: "latest.json", download_count: 3, updated_at: manifestUpdated },
    ],
    calls: [],
    fail: null,
    refName: "refs/heads/stats",
    treeEntries: null,
    responses: new Map(),
  };
  const blobs = new Map([[sha(3), csv]]);
  const trees = new Map([[sha(2), sha(3)]]);
  const commits = new Map([[sha(1), { tree: sha(2), parents: [] }]]);
  let id = 100;
  const response = (body, status = 200) => new Response(JSON.stringify(body), { status });
  const ref = () => ({ ref: state.refName, object: { type: "commit", sha: state.head } });
  const fetchImpl = async (url, options) => {
    const parsed = new URL(url);
    assert.equal(parsed.origin, "https://api.github.com");
    assert.ok(parsed.pathname.startsWith("/repos/example/ibis/"));
    assert.equal(options.headers.Authorization, "Bearer synthetic-token");
    const path = parsed.pathname.slice("/repos/example/ibis/".length) + parsed.search;
    const method = options.method;
    const body = options.body ? JSON.parse(options.body) : undefined;
    state.calls.push({ method, path, body });
    if (state.fail === `${method} ${path}`) return response({ message: "synthetic failure" }, 503);
    if (state.responses.has(`${method} ${path}`)) return response(state.responses.get(`${method} ${path}`));
    if (method === "GET" && path === "releases/tags/stable") return response(state.release);
    if (method === "GET" && path.startsWith("releases/12/assets?")) {
      const page = Number(parsed.searchParams.get("page"));
      assert.equal(parsed.searchParams.get("per_page"), "100");
      return response(state.assets.slice((page - 1) * 100, page * 100));
    }
    if (method === "GET" && path === "git/ref/heads/stats") return state.head ? response(ref()) : response({}, 404);
    if (method === "GET" && path.startsWith("git/commits/")) {
      const hash = path.slice("git/commits/".length);
      const commit = commits.get(hash);
      assert.ok(commit);
      return response({ sha: hash, tree: { sha: commit.tree }, parents: commit.parents.map((sha) => ({ sha })) });
    }
    if (method === "GET" && path.startsWith("git/trees/")) {
      const hash = path.slice("git/trees/".length);
      return response({ sha: hash, truncated: false, tree: state.treeEntries ?? [{ path: "counts.csv", type: "blob", mode: "100644", sha: trees.get(hash) }] });
    }
    if (method === "GET" && path.startsWith("git/blobs/")) {
      const hash = path.slice("git/blobs/".length);
      return response({ sha: hash, encoding: "base64", content: Buffer.from(blobs.get(hash)).toString("base64") });
    }
    if (method === "POST" && path === "git/blobs") {
      assert.equal(body.encoding, "utf-8");
      const hash = sha(id++);
      blobs.set(hash, body.content);
      return response({ sha: hash }, 201);
    }
    if (method === "POST" && path === "git/trees") {
      assert.deepEqual(body.tree.map(({ path, mode, type }) => ({ path, mode, type })), [{ path: "counts.csv", mode: "100644", type: "blob" }]);
      const hash = sha(id++);
      trees.set(hash, body.tree[0].sha);
      return response({ sha: hash }, 201);
    }
    if (method === "POST" && path === "git/commits") {
      const hash = sha(id++);
      commits.set(hash, body);
      return response({ sha: hash }, 201);
    }
    if (method === "POST" && path === "git/refs" || method === "PATCH" && path === "git/refs/heads/stats") {
      if (method === "POST") {
        assert.equal(body.ref, "refs/heads/stats");
        if (state.head) return response({}, 422);
        assert.deepEqual(commits.get(body.sha).parents, [], "first commit is orphaned");
      } else {
        assert.equal(body.force, false);
        if (commits.get(body.sha).parents[0] !== state.head) return response({}, 422);
      }
      state.head = body.sha;
      state.csv = blobs.get(trees.get(commits.get(state.head).tree));
      return response(ref(), method === "POST" ? 201 : 200);
    }
    assert.fail(`unexpected API call: ${method} ${path}`);
  };
  const run = (when = monday) => recordCounts({ repository: "example/ibis", token: "synthetic-token", now: new Date(when), fetchImpl });
  return { state, run };
}

test("first run creates an orphan stats branch with a UTC row for every stable asset", async () => {
  const { state, run } = github();
  await run("2026-10-25T21:15:00-04:00");
  assert.equal(state.csv, header + [
    `2026-10-26,Ibis-synthetic.dmg,40,${packageUpdated}`,
    `2026-10-26,Ibis-synthetic.app.tar.gz,9,${packageUpdated}`,
    `2026-10-26,latest.json,3,${manifestUpdated}`,
    "",
  ].join("\n"));
  assert.equal(state.calls.at(-1).path, "git/refs");
});

test("formula or control-leading asset names refuse visibly before any write", async () => {
  const names = ["=1+1", "+1+1", "-1+1", "@SUM(1,1)", ...Array.from({ length: 32 }, (_, i) => `${String.fromCharCode(i)}Ibis.dmg`), "\x7fIbis.dmg", "\x85Ibis.dmg", "\x9fIbis.dmg"];
  for (const csv of [null, header]) {
    for (const name of names) {
      const { state, run } = github({ csv });
      const previousHead = state.head;
      state.assets[1].name = name;
      await assert.rejects(run(), /unsafe.*asset name/i);
      assert.equal(state.csv, csv);
      assert.equal(state.head, previousHead);
      assert.ok(state.calls.every(({ method }) => method === "GET"));
    }
  }
});

test("malformed release or assets refuse without any write", async () => {
  for (const corrupt of [
    (s) => { s.release = null; },
    (s) => { s.release.tag_name = "nightly"; },
    (s) => { s.release.draft = true; },
    (s) => { s.release.id = "12"; },
    (s) => { s.assets = []; },
    (s) => { s.assets[0].download_count = -1; },
    (s) => { s.assets[0].download_count = "40"; },
    (s) => { s.assets[0].id = 0; },
    (s) => { s.assets[0].name = ""; },
    (s) => { s.assets[0].name = s.assets[1].name; },
    (s) => { s.assets[0].id = s.assets[1].id; },
    (s) => { delete s.assets[0].updated_at; },
    (s) => { s.assets[0].updated_at = "not a timestamp"; },
    (s) => { s.assets[0].updated_at = 123; },
    (s) => { s.assets[2].updated_at = null; },
  ]) {
    const { state, run } = github({ csv: header });
    corrupt(state);
    await assert.rejects(run());
    assert.equal(state.csv, header);
    assert.ok(state.calls.every(({ method }) => method === "GET"));
  }
});

test("failed API reads leave earlier CSV bytes and the stats ref unchanged", async () => {
  for (const fail of ["GET releases/tags/stable", "GET releases/12/assets?per_page=100&page=1", "GET git/ref/heads/stats", `GET git/blobs/${sha(3)}`]) {
    const { state, run } = github({ csv: header });
    state.fail = fail;
    await assert.rejects(run());
    assert.equal(state.csv, header);
    assert.equal(state.head, sha(1));
  }
});

test("a rerun anywhere in the same Monday-to-Sunday UTC week is a no-op", async () => {
  const { state, run } = github();
  await run();
  const previous = state.csv;
  state.calls = [];
  state.assets[0].download_count = 55;
  await run("2026-11-01T23:59:59Z");
  assert.equal(state.csv, previous);
  assert.ok(state.calls.every(({ method }) => method === "GET"));
  await run(nextMonday);
  assert.ok(state.csv.startsWith(previous));
  assert.ok(state.csv.includes(`2026-11-02,Ibis-synthetic.dmg,55,${packageUpdated}\n`));
});

test("replacement counters append with their own timestamps regardless of latest.json ordering", async () => {
  for (const index of [0, 1]) {
    for (const updated_at of [packageUpdated, manifestUpdated, "2026-11-01T23:58:00Z"]) {
      const { state, run } = github();
      await run();
      const previous = state.csv;
      const replacement = { ...state.assets[index], id: 30, download_count: 2, updated_at };
      state.assets[index] = replacement;
      await run(nextMonday);
      assert.ok(state.csv.startsWith(previous));
      assert.ok(state.csv.includes(`2026-11-02,${replacement.name},2,${updated_at}\n`));
      assert.ok(state.csv.includes(`2026-11-02,latest.json,3,${manifestUpdated}\n`));
    }
  }
});

test("commas, quotes, CR and newlines in raw asset names round-trip through weekly deduplication", async () => {
  const { state, run } = github();
  state.assets[0].name = 'Ibis,"synthetic"\r\npackage.dmg';
  await run();
  const previous = state.csv;
  assert.ok(previous.includes(`2026-10-26,"Ibis,""synthetic""\r\npackage.dmg",40,${packageUpdated}\n`));
  await run();
  assert.equal(state.csv, previous);
  await run(nextMonday);
  assert.ok(state.csv.startsWith(previous));
});

test("snapshots need neither latest.json nor a numbered release title", async () => {
  const { state, run } = github();
  state.assets.pop();
  state.release.name = "Ibis update feed";
  await run();
  assert.ok(state.csv.includes(`2026-10-26,Ibis-synthetic.dmg,40,${packageUpdated}\n`));
  assert.equal(state.csv.split("\n").length, 4);
});

test("all asset pages are recorded", async () => {
  const { state, run } = github();
  state.assets = Array.from({ length: 101 }, (_, i) => ({ id: 1000 + i, name: `synthetic-${i}.dmg`, download_count: i, updated_at: packageUpdated }));
  await run();
  assert.equal(state.csv.split("\n").length, 103);
  assert.ok(state.csv.includes(`synthetic-100.dmg,100,${packageUpdated}\n`));
});

test("ref confusion, unrelated trees and damaged CSV are refused before writes", async () => {
  for (const corrupt of [
    (s) => { s.refName = "refs/heads/main"; },
    (s) => { s.refName = "refs/tags/stats"; },
    (s) => { s.treeEntries = [{ path: "README.md", mode: "100644", type: "blob", sha: sha(3) }]; },
    (s) => { s.treeEntries = [{ path: "counts.csv", mode: "120000", type: "blob", sha: sha(3) }]; },
  ]) {
    const { state, run } = github({ csv: header });
    corrupt(state);
    await assert.rejects(run());
    assert.equal(state.csv, header);
    assert.ok(state.calls.every(({ method }) => method === "GET"));
  }
  for (const csv of ["", "wrong,header\n", header + `2026-02-30,file.dmg,1,${packageUpdated}\n`, header + '2026-10-19,"broken,1\n', header + `2026-10-19,file.dmg,-1,${packageUpdated}\n`, header + `2026-10-19,file.dmg,"1\n",${packageUpdated}\n`, header + `2026-10-19,file.dmg,1,${packageUpdated}`, header + "2026-10-19,file.dmg,1,invalid\n", header + `2026-10-19,+1+1,1,${packageUpdated}\n`]) {
    const { state, run } = github({ csv });
    await assert.rejects(run());
    assert.equal(state.csv, csv);
  }
});

test("malformed API refs, trees and blobs never become a new empty history", async () => {
  for (const [path, payload] of [
    ["git/ref/heads/stats", null],
    ["git/ref/heads/stats", []],
    ["git/ref/heads/stats", { ref: "refs/heads/stats", object: { type: "tag", sha: sha(1) } }],
    ["git/ref/heads/stats", { ref: "refs/heads/stats", object: { type: "commit", sha: sha(1) + "\n" } }],
    [`git/trees/${sha(2)}`, { sha: sha(2), truncated: true, tree: [] }],
    [`git/blobs/${sha(3)}`, { sha: sha(3), encoding: "base64", content: "invalid!" }],
  ]) {
    const { state, run } = github({ csv: header });
    state.responses.set(`GET ${path}`, payload);
    await assert.rejects(run());
    assert.equal(state.csv, header);
    assert.ok(state.calls.every(({ method }) => method === "GET"));
  }
});

test("failed writes and competing first runs cannot replace earlier history", async () => {
  for (const fail of ["POST git/blobs", "POST git/trees", "POST git/commits", "PATCH git/refs/heads/stats"]) {
    const { state, run } = github({ csv: header });
    state.fail = fail;
    await assert.rejects(run());
    assert.equal(state.csv, header);
    assert.equal(state.head, sha(1));
  }
  const { state, run } = github();
  const results = await Promise.allSettled([run(), run()]);
  assert.equal(results.filter(({ status }) => status === "fulfilled").length, 1);
  assert.equal(state.csv.split("\n").length, 5);
});

test("a delayed older snapshot cannot overwrite a newer week or a competing append", async () => {
  const { state, run } = github();
  await run(nextMonday);
  const previous = state.csv;
  await assert.rejects(run(monday));
  assert.equal(state.csv, previous);
  const results = await Promise.allSettled([run("2026-11-09T00:00:00Z"), run("2026-11-09T00:00:00Z")]);
  assert.equal(results.filter(({ status }) => status === "fulfilled").length, 1);
  assert.ok(state.csv.startsWith(previous));
  assert.equal(state.csv.split("\n").length, 8);
});

test("workflow limits credentials and permissions and CI classifies it conservatively", () => {
  const workflow = readFileSync(new URL("../.github/workflows/release-counts.yml", import.meta.url), "utf8");
  assert.match(workflow, /^permissions: \{\}$/m);
  assert.equal(workflow.match(/contents: write/g)?.length, 1);
  assert.match(workflow, /cancel-in-progress: false/);
  assert.match(workflow, /github.ref == 'refs\/heads\/main'/);
  assert.match(workflow, /persist-credentials: false/);
  assert.match(workflow, /workflow_dispatch:/);
  assert.match(workflow, /cron: '17 3 \* \* 1'/);
  assert.match(workflow, /run: node scripts\/release-counts.mjs/);
  for (const action of workflow.matchAll(/uses: (\S+)/g)) assert.match(action[1], /@[a-f0-9]{40}$/);
  assert.doesNotMatch(workflow, /pull_request:|secrets\.|inputs\.|force: true/);
  assert.deepEqual(classify("pull_request", 1, [{ filename: ".github/workflows/release-counts.yml", status: "added" }]), { native: true, web: true });
});

test("the CLI refuses dispatches from other refs, unexpected events and arguments before network access", () => {
  const script = fileURLToPath(new URL("./release-counts.mjs", import.meta.url));
  for (const [ref, event, args] of [
    ["refs/heads/stats", "workflow_dispatch", []],
    ["refs/tags/main", "workflow_dispatch", []],
    ["refs/heads/main", "pull_request", []],
    ["refs/heads/main", "workflow_dispatch", ["extra"]],
  ]) {
    const result = spawnSync(process.execPath, [script, ...args], {
      env: { ...process.env, GITHUB_REF: ref, GITHUB_EVENT_NAME: event, GH_TOKEN: "", GITHUB_REPOSITORY: "" },
      encoding: "utf8", timeout: 5000,
    });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Run this workflow from main/);
    assert.equal(result.stdout, "");
  }
});
