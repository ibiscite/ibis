import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { test } from "node:test";

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), "utf8");
const pathKey = Object.keys(process.env).find((key) => key.toLowerCase() === "path") ?? "PATH";
const workflow = read(".github/workflows/preview.yml");
const job = (name) => workflow.split(`\n  ${name}:\n`)[1]?.split(/^  [\w-]+:\n/m)[0];
const step = (name, prefix) => {
  const text = job(name)?.split(/^      - name: /m).find((part) => part.startsWith(prefix));
  assert.ok(text, `${name}: ${prefix}`);
  return text.split("        run: |\n")[1].replace(/^          /gm, "");
};

// Execute the workflow's shell and real version comparator with fake GitHub and jq.
function fakeGithub() {
  const fs = require("node:fs");
  const args = process.argv.slice(2);
  const state = JSON.parse(fs.readFileSync("github.json"));
  state.calls.push(args);
  const save = () => fs.writeFileSync("github.json", JSON.stringify(state));
  const value = (flag) => args[args.indexOf(flag) + 1];
  const response = (status, body, network = false) => {
    if (network) { console.log("partial response"); process.exit(1); }
    console.log(`HTTP/2.0 ${status} Test\r\nContent-Type: application/json\r\n\r\n${body}`);
    if (status !== 200) process.exit(1);
  };
  save();
  if (args[0] === "api") {
    if (args[1] === "repos/example/ibis/git/ref/tags/nightly") {
      const status = state.refStatus ?? (state.currentSha ? 200 : 404);
      const body = state.refBody ?? JSON.stringify({ object: { type: "commit", sha: state.currentSha } });
      if (args.includes("--include")) response(status, body, state.refNetwork);
      else {
        console.log(state.currentSha ?? "");
        if (state.refNetwork || status !== 200) process.exit(1);
      }
      return;
    }
    if (args[1] === "repos/example/ibis/releases/tags/stable") {
      // GitHub's tag lookup does not expose drafts, even with authentication.
      response(state.feedStatus ?? (state.releases.stable && !state.releases.stable.isDraft ? 200 : 404), JSON.stringify({ id: 12 }), state.feedNetwork);
      return;
    }
    const releasesPage = /^repos\/example\/ibis\/releases\?per_page=100&page=(\d+)$/.exec(args[1]);
    if (releasesPage) {
      const page = Number(releasesPage[1]);
      const releases = state.releasePages?.[page - 1] ?? Object.entries(state.releases).map(([tag, release], index) => ({
        id: tag === "stable" ? 12 : 20 + index, tag_name: tag, draft: release.isDraft,
      }));
      response(state.releaseStatuses?.[page - 1] ?? 200,
        state.releaseBodies?.[page - 1] ?? JSON.stringify(releases), state.releaseNetworks?.[page - 1]);
      return;
    }
    if (args[1].startsWith("repos/example/ibis/releases/12/assets?per_page=100&page=")) {
      const page = Number(args[1].split("page=").at(-1));
      const assets = state.assetPages?.[page - 1] ?? Object.keys(state.releases.stable.assets).map((name) => ({ name }));
      response(state.assetStatuses?.[page - 1] ?? state.assetStatus ?? 200, JSON.stringify(assets));
      return;
    }
    if (args[1].startsWith("repos/example/ibis/compare/")) {
      if (args[1] !== `repos/example/ibis/compare/${state.currentSha}...${"a".repeat(40)}`) process.exit(2);
      console.log(state.relation ?? "ahead");
      if (state.failCompare) process.exit(1);
      return;
    }
    // Nightly publication moves its tag only after replacing the assets.
    if (args.includes("--method")) return;
    if (args[1] !== "repos/example/ibis/releases/latest") process.exit(1);
    console.log(state.latest);
    if (state.failLatest) process.exit(1); // Even plausible partial output is not success.
    return;
  }
  if (args[0] !== "release") process.exit(2);
  const [, command, tag] = args;
  let release = state.releases[tag];
  if (command === "view") {
    if (!release) process.exit(1);
    console.log(args.includes("--jq") ? Object.keys(release.assets).join("\n") : JSON.stringify(release));
    return;
  }
  if (command === "download") {
    if (!release?.assets["latest.json"]) process.exit(1);
    console.log(release.assets["latest.json"]);
    if (state.feedDownloadFailure) process.exit(1);
    return;
  }
  if (command === "create") {
    if (release) process.exit(1);
    release = state.releases[tag] = { isDraft: args.includes("--draft"), prerelease: false, assets: {} };
  } else if (!release) process.exit(1);
  if (command === "delete-asset" && state.failCleanup || command === "edit" && state.failFinalize) {
    state.failCleanup = state.failFinalize = false;
    save();
    process.exit(1);
  }
  if (command === "upload" || command === "create") {
    if (state.failUpload) process.exit(1);
    for (const path of args.filter((arg) => /^(dist|feed)\//.test(arg))) {
      release.assets[path.split("/").at(-1)] = fs.readFileSync(path, "utf8");
    }
    if (state.failAfterFeedUpload && args.includes("feed/latest.json")) {
      state.failAfterFeedUpload = false;
      save(); // The upload succeeded remotely before its client reported failure.
      process.exit(1);
    }
  }
  if (command === "edit" || command === "create") {
    if (args.includes("--draft=false")) release.isDraft = false;
    if (args.includes("--prerelease")) release.prerelease = true;
    if (args.includes("--prerelease=false")) release.prerelease = false;
    if (args.includes("--latest")) state.latest = tag;
    if (args.includes("--title")) release.title = value("--title");
    if (args.includes("--notes-file")) release.body = fs.readFileSync(value("--notes-file"), "utf8");
  } else if (command === "delete-asset") delete release.assets[args[3]];
  else if (command !== "upload") process.exit(2);
  save();
}

function fakeJq() {
  const fs = require("node:fs");
  const [option, field, file, ...extra] = process.argv.slice(2);
  if (option !== "-r" || ![".body", ".isDraft", ".version"].includes(field) || extra.length) {
    console.error("unsupported fixture jq arguments");
    process.exit(2);
  }
  const value = JSON.parse(fs.readFileSync(file ?? 0, "utf8"))[field.slice(1)];
  console.log(typeof value === "string" ? value : JSON.stringify(value ?? null));
}

function fixture(t, initial = {}) {
  const dir = mkdtempSync(join(tmpdir(), "ibis-release-config-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  for (const name of ["scripts", "dist", "feed"]) mkdirSync(join(dir, name));
  for (const name of ["update-feed.mjs", "release-notes.mjs", "publication-guard.mjs"]) {
    copyFileSync(new URL(name, import.meta.url), join(dir, "scripts", name));
  }
  writeFileSync(join(dir, "gh"), `#!/usr/bin/env node\n(${fakeGithub.toString()})();\n`, { mode: 0o755 });
  // The publication step's version command is unrelated to these guards and
  // would read Git. Keep the real comparator for numbered release checks.
  writeFileSync(join(dir, "node"), `#!/bin/sh\nif [ "$1" = scripts/update-feed.mjs ] && [ "$2" = version ]; then\n  echo 0.2.4-nightly.20261031000000\nelse\n  exec '${process.execPath.replaceAll("'", "'\\''")}' "$@"\nfi\n`, { mode: 0o755 });
  writeFileSync(join(dir, "jq"), `#!/usr/bin/env node\n(${fakeJq.toString()})();\n`, { mode: 0o755 });
  writeFileSync(join(dir, "dist/Ibis-synthetic.zip"), "invented package");
  const hash = createHash("sha256").update("invented package").digest("hex");
  writeFileSync(join(dir, "dist/SHA256SUMS-synthetic.txt"), `${hash}  Ibis-synthetic.zip\n`);
  writeFileSync(join(dir, "feed/latest.json"), JSON.stringify({ version: "0.2.3" }));
  const draft = { isDraft: true, prerelease: true, body: "### Bug Fixes\n* invented fix", assets: {} };
  writeFileSync(join(dir, "github.json"), JSON.stringify({ latest: "v0.2.2", releases: { "v0.2.3": draft }, calls: [], ...initial }));
  const state = () => JSON.parse(readFileSync(join(dir, "github.json"), "utf8"));
  const run = (name, prefix, env = {}) => spawnSync("bash", ["--noprofile", "--norc", "-e", "-o", "pipefail", "-c", step(name, prefix)], {
    cwd: dir, encoding: "utf8", timeout: 15_000,
    env: { ...process.env, [pathKey]: `${dir}${delimiter}${process.env[pathKey] ?? ""}`, REPO: "example/ibis", TAG: "v0.2.3", ACTOR: "maintainer", FEED: "stable", SHA: "a".repeat(40), ...env },
  });
  return { dir, state, run };
}

test("numbered publishes and stable-feed updates queue without interleaving or cancelling an active job", () => {
  const publish = job("publish");
  assert.match(publish, /^    concurrency:\n      group: publish-numbered-release\n      queue: max\n      cancel-in-progress: false$/m);
  assert.doesNotMatch(publish, /^\s+cancel-in-progress:\s*true\b/m);
  assert.match(job("stable-feed"), /^    concurrency:\n      group: publish-stable-feed\n      queue: max\n      cancel-in-progress: false$/m);
});

test("numbered releases publish fully with a versioned title and explicit Latest choice", (t) => {
  for (const releases of [undefined, {}]) for (const latest of ["v0.2.2", "v0.2.3", "v0.2.10"]) {
    const f = fixture(t, { latest, ...(releases ? { releases } : {}) });
    const result = f.run("publish", "Publish the verified");
    assert.equal(result.status, 0, result.stdout + result.stderr);
    const state = f.state(), release = state.releases["v0.2.3"];
    assert.equal(release.isDraft, false);
    assert.equal(release.prerelease, false);
    assert.equal(release.title, "Ibis 0.2.3");
    assert.equal(state.latest, latest === "v0.2.10" ? latest : "v0.2.3");
    const edit = state.calls.find((args) => args.includes("--draft=false"));
    assert.ok(edit.includes(latest === "v0.2.10" ? "--latest=false" : "--latest"));
  }
});

test("failed Latest I/O or package upload leaves a numbered draft unpublished", (t) => {
  for (const initial of [{ failLatest: true }, { failLatest: true, releases: {} }, { failUpload: true }]) {
    const f = fixture(t, initial);
    assert.notEqual(f.run("publish", "Publish the verified").status, 0);
    assert.notEqual(f.state().releases["v0.2.3"]?.isDraft, false);
    assert.ok(!f.state().calls.some((args) => args.includes("--draft=false")));
    assert.equal(f.state().latest, "v0.2.2");
    if (initial.failLatest) assert.deepEqual(f.state().calls, [["api", "repos/example/ibis/releases/latest", "--jq", ".tag_name"]]);
  }
});

test("public numbered releases and bot tags without a draft still refuse publication", (t) => {
  for (const releases of [{ "v0.2.3": { isDraft: false, assets: {} } }, {}]) {
    const f = fixture(t, { releases });
    const result = f.run("publish", "Publish the verified", { ACTOR: "release[bot]" });
    assert.notEqual(result.status, 0);
    assert.match(result.stdout, /already public|No draft release/);
    assert.ok(f.state().calls.every((args) => args[0] === "api" || args[1] === "view"));
  }
});

test("stable-feed alone replaces assets, uploads the feed last and never moves the fixed tag", (t) => {
  const f = fixture(t, { releases: { stable: { isDraft: false, assets: { "latest.json": '{"version":"0.2.2"}', "old.zip": "old" } } } });
  for (let rerun = 0; rerun < 2; rerun++) {
    const result = f.run("stable-feed", "Move the stable update feed to this release");
    assert.equal(result.status, 0, result.stdout + result.stderr);
    const state = f.state(), feed = state.releases.stable;
    assert.equal(feed.prerelease, true);
    assert.equal(feed.title, "Ibis update feed");
    assert.equal(feed.assets["Ibis-synthetic.zip"], "invented package");
    assert.equal(JSON.parse(feed.assets["latest.json"]).version, "0.2.3");
    assert.ok(!("old.zip" in feed.assets));
    assert.match(feed.body, /\*\*Not a release\.\*\*.*\[Ibis 0\.2\.3\]\(https:\/\/github.com\/example\/ibis\/releases\/tag\/v0\.2\.3\)/);
    assert.equal(state.latest, "v0.2.2");
    assert.ok(state.calls.every((args) => !args.includes("--method") && args[1] !== "create"));
    const uploads = state.calls.filter((args) => args[1] === "upload");
    assert.ok(uploads.at(-2).includes("dist/Ibis-synthetic.zip"));
    assert.ok(uploads.at(-1).includes("feed/latest.json"));
    assert.ok(state.calls.at(-1).includes("--latest=false"));
  }
});

test("delayed older runs cannot move Latest or the stable feed backwards", (t) => {
  const f = fixture(t, { latest: "v0.2.10", releases: {
    "v0.2.3": { isDraft: true, prerelease: true, body: "", assets: {} },
    stable: { isDraft: false, assets: { "latest.json": '{"version":"0.2.10"}' } },
  } });
  assert.equal(f.run("publish", "Publish the verified").status, 0);
  assert.equal(f.state().latest, "v0.2.10");
  assert.equal(f.state().releases["v0.2.3"].prerelease, false);
  assert.notEqual(f.run("publish", "Publish the verified").status, 0, "a rerun cannot overwrite the now-public older release");
  const callsBeforeFeed = f.state().calls.length;
  for (let rerun = 0; rerun < 2; rerun++) {
    assert.equal(f.run("stable-feed", "Move the stable update feed to this release").status, 0);
    assert.equal(f.state().latest, "v0.2.10");
    assert.equal(JSON.parse(f.state().releases.stable.assets["latest.json"]).version, "0.2.10");
    assert.ok(f.state().calls.slice(callsBeforeFeed).every((args) => args[0] === "api" || args[1] === "download"));
  }
});

test("stable feed publication permits confirmed absence or an older parsed feed", (t) => {
  for (const assets of [undefined, { "old.zip": "old" }, { "latest.json": '{"version":"0.2.2"}' }]) {
    const f = fixture(t, { releases: assets === undefined ? {} : { stable: { isDraft: false, assets } } });
    const result = f.run("stable-feed", "Move the stable update feed to this release");
    assert.equal(result.status, 0, result.stdout + result.stderr);
    assert.equal(JSON.parse(f.state().releases.stable.assets["latest.json"]).version, "0.2.3");
  }
});

test("stable feed checks authenticated draft assets before replacing permanent downloads", (t) => {
  const assets = { "latest.json": '{"version":"0.2.10"}', "Ibis-synthetic.zip": "newer package" };
  const f = fixture(t, { releases: { stable: { isDraft: true, assets } } });
  const result = f.run("stable-feed", "Move the stable update feed to this release");
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.deepEqual(f.state().releases.stable.assets, assets);
  assert.equal(f.state().releases.stable.isDraft, true);
  assert.ok(f.state().calls.some((args) => args[1] === "repos/example/ibis/releases?per_page=100&page=1"));
});

test("stable draft discovery reads every release page and refuses incomplete or invalid listings", (t) => {
  const firstPage = Array.from({ length: 100 }, (_, index) => ({ id: 100 + index, tag_name: `synthetic-${index}`, draft: false }));
  const stable = { id: 12, tag_name: "stable", draft: true };
  for (const failure of [
    {}, { releaseStatuses: [200, 500] }, { releaseStatuses: [200, 404] },
    { releaseNetworks: [false, true] }, { releaseBodies: [undefined, "{"] },
    { releasePages: [firstPage, { unexpected: "response" }] },
    { releasePages: [firstPage, [{}]] },
    { releasePages: [[stable, ...firstPage.slice(1)], []], releaseStatuses: [200, 500] },
    { releasePages: [[stable, ...firstPage.slice(1)], [stable]] },
  ]) {
    const assets = { "latest.json": '{"version":"0.2.10"}', "Ibis-synthetic.zip": "newer package" };
    const f = fixture(t, { releases: { stable: { isDraft: true, assets } }, releasePages: [firstPage, [stable]], ...failure });
    const result = f.run("stable-feed", "Move the stable update feed to this release");
    if (Object.keys(failure).length === 0) assert.equal(result.status, 0, result.stdout + result.stderr);
    else assert.notEqual(result.status, 0, JSON.stringify(failure));
    assert.deepEqual(f.state().releases.stable.assets, assets);
    assert.equal(f.state().releases.stable.isDraft, true);
    assert.equal(f.state().calls.filter((args) => args[1]?.includes("/releases?")).length, 2);
    assert.ok(f.state().calls.every((args) => args[0] === "api" || args[1] === "download"));
  }
});

test("stable release absence requires a complete successful release listing", (t) => {
  const firstPage = Array.from({ length: 100 }, (_, index) => ({ id: 100 + index, tag_name: `synthetic-${index}`, draft: false }));
  for (const status of [200, 500]) {
    const f = fixture(t, { releases: {}, releasePages: [firstPage, []], releaseStatuses: [200, status] });
    const result = f.run("stable-feed", "Move the stable update feed to this release");
    if (status === 200) {
      assert.equal(result.status, 0, result.stdout + result.stderr);
      assert.equal(f.state().releases.stable.isDraft, false);
      assert.equal(JSON.parse(f.state().releases.stable.assets["latest.json"]).version, "0.2.3");
    } else {
      assert.notEqual(result.status, 0);
      assert.deepEqual(f.state().releases, {});
      assert.ok(f.state().calls.every((args) => args[0] === "api"));
    }
    assert.equal(f.state().calls.filter((args) => args[1]?.includes("/releases?")).length, 2);
  }
});

test("equal-version retries finish cleanup and finalization after the feed upload", (t) => {
  for (const isDraft of [false, true]) for (const failure of ["failAfterFeedUpload", "failCleanup", "failFinalize"]) {
    const f = fixture(t, { releases: { stable: { isDraft, assets: { "latest.json": '{"version":"0.2.2"}', "old.zip": "old" } } }, [failure]: true });
    const interrupted = f.run("stable-feed", "Move the stable update feed to this release");
    assert.notEqual(interrupted.status, 0, failure);
    assert.equal(JSON.parse(f.state().releases.stable.assets["latest.json"]).version, "0.2.3", "the candidate feed is already uploaded");
    assert.equal(f.state().releases.stable.isDraft, isDraft);
    assert.equal("old.zip" in f.state().releases.stable.assets, failure !== "failFinalize");
    const callsBeforeRetry = f.state().calls.length;
    const retried = f.run("stable-feed", "Move the stable update feed to this release");
    assert.equal(retried.status, 0, retried.stdout + retried.stderr);
    const release = f.state().releases.stable;
    assert.equal(release.isDraft, false);
    assert.equal(release.title, "Ibis update feed");
    assert.equal("old.zip" in release.assets, false);
    assert.equal(release.assets["Ibis-synthetic.zip"], "invented package");
    assert.equal(JSON.parse(release.assets["latest.json"]).version, "0.2.3");
    assert.ok(f.state().calls.slice(callsBeforeRetry).some((args) => args[1] === "edit" && args.includes("--draft=false")));
  }
});

test("stable feed must read later asset pages and refuses an incomplete listing", (t) => {
  for (const status of [200, 500]) {
    const assets = { "latest.json": '{"version":"0.2.10"}', "Ibis-synthetic.zip": "newer package" };
    const f = fixture(t, { releases: { stable: { isDraft: false, assets } },
      assetPages: [Array.from({ length: 100 }, (_, index) => ({ name: `old-${index}.zip` })), [{ name: "latest.json" }]],
      assetStatuses: [200, status],
    });
    const result = f.run("stable-feed", "Move the stable update feed to this release");
    if (status === 200) assert.equal(result.status, 0, result.stdout + result.stderr);
    else assert.notEqual(result.status, 0);
    assert.deepEqual(f.state().releases.stable.assets, assets);
    assert.equal(f.state().calls.filter((args) => args[1]?.includes("/assets?")).length, 2);
    assert.ok(f.state().calls.every((args) => args[0] === "api" || args[1] === "download"));
  }
});

test("stable feed read failures stop before changing permanent installers or latest.json", (t) => {
  for (const failure of [
    { feedStatus: 500 }, { feedStatus: 403 }, { feedNetwork: true },
    { feedStatus: 404, feedNetwork: true }, { feedDownloadFailure: true }, { assetStatus: 500 },
    { assetPages: [[{}]] }, { assetPages: [{ unexpected: "response" }] },
    ...["{", "null", "{}", '{"version":null}', '{"version":""}', '{"version":"invalid"}'].map((body) => ({ body })),
  ]) {
    const assets = { "latest.json": failure.body ?? '{"version":"0.2.10"}', "Ibis-synthetic.zip": "newer package" };
    const f = fixture(t, { ...failure, releases: { stable: { isDraft: false, assets } } });
    const result = f.run("stable-feed", "Move the stable update feed to this release");
    assert.notEqual(result.status, 0, JSON.stringify(failure));
    assert.deepEqual(f.state().releases.stable.assets, assets);
    assert.ok(f.state().calls.every((args) => args[0] === "api" || args[1] === "download"), "no publication mutations after uncertainty");
  }
});

test("nightly only publishes after a successful ahead comparison or confirmed ref absence", (t) => {
  for (const currentSha of [undefined, "b".repeat(40)]) {
    const f = fixture(t, { currentSha });
    const result = f.run("nightly", "Replace the nightly pre-release", { TAG: "nightly" });
    assert.equal(result.status, 0, result.stdout + result.stderr);
    assert.equal(f.state().releases.nightly.isDraft, false);
    assert.equal(JSON.parse(f.state().releases.nightly.assets["latest.json"]).version, "0.2.3");
    assert.equal(f.state().calls.some((args) => args[1]?.startsWith("repos/example/ibis/compare/")), currentSha !== undefined);
  }
});

test("nightly read failures and malformed ref responses stop before any publication mutation", (t) => {
  for (const failure of [
    { refStatus: 500 }, { refStatus: 403 }, { refNetwork: true },
    { refStatus: 404, refNetwork: true }, { failCompare: true },
    { relation: "unknown" },
    ...["{", "null", "{}", '{"object":{"sha":""}}', '{"object":{"sha":"invalid"}}',
      JSON.stringify({ object: { type: "tag", sha: "b".repeat(40) } }),
      JSON.stringify({ object: { type: "commit", sha: ["b".repeat(40)] } }),
    ].map((refBody) => ({ refBody })),
  ]) {
    const assets = { "latest.json": '{"version":"0.2.10"}', "Ibis-synthetic.zip": "newer package" };
    const f = fixture(t, { currentSha: "b".repeat(40), ...failure, releases: { nightly: { isDraft: false, assets } } });
    const result = f.run("nightly", "Replace the nightly pre-release", { TAG: "nightly" });
    assert.notEqual(result.status, 0, JSON.stringify(failure));
    assert.deepEqual(f.state().releases.nightly.assets, assets);
    assert.ok(f.state().calls.every((args) => args[0] === "api" && !args.includes("--method")), "no publication mutations after uncertainty");
  }
});

test("nightly leaves a newer, equal or diverged nightly untouched", (t) => {
  for (const relation of ["behind", "identical", "diverged"]) {
    const assets = { "latest.json": '{"version":"0.2.10"}', "Ibis-synthetic.zip": "newer package" };
    const f = fixture(t, { currentSha: "b".repeat(40), relation, releases: { nightly: { isDraft: false, assets } } });
    const result = f.run("nightly", "Replace the nightly pre-release", { TAG: "nightly" });
    assert.equal(result.status, 0, result.stdout + result.stderr);
    assert.deepEqual(f.state().releases.nightly.assets, assets);
    assert.equal(f.state().calls.length, 2, "only the ref and comparison may be read");
  }
});

test("nightly has a fixed title, never Latest, and links to the numbered release", () => {
  const nightly = job("nightly");
  for (const command of ["create", "edit"]) {
    const line = nightly.split("\n").find((line) => line.includes(`gh release ${command}`));
    assert.match(line, /--title "Ibis Nightly"/);
    if (command === "edit") assert.match(line, /--latest=false/);
  }
  assert.match(nightly, /\*\*Ibis Nightly \$version\*\*/);
  assert.ok(nightly.includes("[newest numbered preview release](https://github.com/$REPO/releases/latest)"));
});

test("release flags leave versioning and draft tag creation unchanged", () => {
  const config = JSON.parse(read("release-please-config.json"));
  assert.ok(!Object.hasOwn(config, "prerelease"));
  assert.equal(config.draft, true);
  assert.equal(config["force-tag-creation"], true);
  assert.equal(config["release-type"], "simple");
  assert.equal(config["bump-minor-pre-major"], true);
  assert.equal(config["bump-patch-for-minor-pre-major"], true);
  assert.ok(read("website/index.html").includes('Files come from GitHub. <a href="https://github.com/ibiscite/ibis/releases/latest">Release notes</a>'));
  assert.ok(read("README.md").includes("[release](https://github.com/ibiscite/ibis/releases/latest)"));
});

test("Release Please bumps every workspace crate in Cargo.lock", () => {
  const version = read("Cargo.toml").match(/\[workspace\.package\][^[]*?\nversion = "([^"]+)"/)[1];
  const crates = [...read("Cargo.lock").matchAll(/\[\[package\]\]\nname = "(ibis[^"]*)"\nversion = "([^"]+)"\n(?!source)/g)]
    .filter(([, , locked]) => locked === version)
    .map(([, name]) => name)
    .sort();
  const files = JSON.parse(read("release-please-config.json")).packages["."]["extra-files"];
  const bumped = files
    .filter((file) => file.path === "Cargo.lock")
    .map((file) => file.jsonpath.match(/name\.value=='([^']+)'/)[1])
    .sort();
  assert.ok(crates.length > 0, "workspace crates are found in Cargo.lock");
  assert.deepEqual(bumped, crates, "a release must bump every workspace crate or `--locked` builds fail");
});
