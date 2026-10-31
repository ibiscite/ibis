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
  save();
  if (args[0] === "api") {
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
    return;
  }
  if (command === "create") {
    if (release) process.exit(1);
    release = state.releases[tag] = { isDraft: args.includes("--draft"), prerelease: false, assets: {} };
  } else if (!release) process.exit(1);
  if (command === "upload" || command === "create") {
    if (state.failUpload) process.exit(1);
    for (const path of args.filter((arg) => /^(dist|feed)\//.test(arg))) {
      release.assets[path.split("/").at(-1)] = fs.readFileSync(path, "utf8");
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
  for (const name of ["update-feed.mjs", "release-notes.mjs"]) {
    copyFileSync(new URL(name, import.meta.url), join(dir, "scripts", name));
  }
  writeFileSync(join(dir, "gh"), `#!/usr/bin/env node\n(${fakeGithub.toString()})();\n`, { mode: 0o755 });
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

test("numbered publishes serialize the Latest read and publish without cancelling an active job", () => {
  const publish = job("publish");
  assert.match(publish, /^    concurrency:\n      group: publish-numbered-release\n      queue: max\n      cancel-in-progress: false$/m);
  assert.doesNotMatch(publish, /^\s+cancel-in-progress:\s*true\b/m);
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
    assert.ok(state.calls.every((args) => args[0] !== "api" && args[1] !== "create"));
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
    assert.ok(f.state().calls.slice(callsBeforeFeed).every((args) => args[1] === "download"));
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
