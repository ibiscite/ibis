// Read-only gates for permanent downloads. A nightly 404 or complete stable
// release/asset listing confirms absence; uncertainty stops any replacement.
// Prints "publish" or "keep"; lookup/parse failures exit nonzero.
//   node scripts/publication-guard.mjs stable <candidate latest.json>
//   node scripts/publication-guard.mjs nightly
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { compareVersions } from "./update-feed.mjs";

function run(command, args) {
  const result = spawnSync(command, args, {
    encoding: "utf8", stdio: ["ignore", "pipe", "pipe"],
    timeout: 60_000, killSignal: "SIGKILL", maxBuffer: 1024 * 1024,
  });
  if (result.error || result.signal) throw new Error("Publication lookup did not complete.");
  return result;
}

function githubJson(repository, path, missing = false) {
  // gh includes the actual response status even when the API returns an error.
  // Never infer absence from stderr, an empty body or an unsuccessful command.
  const result = run("gh", ["api", `repos/${repository}/${path}`, "--include"]);
  const separator = /\r?\n\r?\n/.exec(result.stdout);
  const status = /^HTTP\/[\d.]+ (\d{3})(?:[ \t][^\r\n]*)?$/.exec(result.stdout.split(/\r?\n/)[0])?.[1];
  if (!separator || !status) throw new Error("GitHub response has no confirmed HTTP status.");
  if (missing && status === "404" && result.status === 1) return undefined;
  if (result.status !== 0 || status !== "200") throw new Error("GitHub lookup failed.");
  return JSON.parse(result.stdout.slice(separator.index + separator[0].length));
}

function nightlyRef(repository) {
  const ref = githubJson(repository, "git/ref/tags/nightly", true);
  if (ref === undefined) return undefined;
  if (ref?.object?.type !== "commit" || typeof ref.object.sha !== "string" || !/^[a-f0-9]{40}$/.test(ref.object.sha)) {
    throw new Error("Nightly ref response has no valid commit.");
  }
  return ref.object.sha;
}

function stableRelease(repository) {
  // The tag endpoint only returns published releases. Its 404 must be followed
  // by a complete authenticated listing, which also exposes unfinished drafts.
  const release = githubJson(repository, "releases/tags/stable", true);
  if (release !== undefined) return release;
  let found;
  for (let page = 1; ; page++) {
    const releases = githubJson(repository, `releases?per_page=100&page=${page}`);
    if (!Array.isArray(releases) || releases.length > 100 || releases.some((entry) =>
      typeof entry?.tag_name !== "string" || !entry.tag_name || !Number.isSafeInteger(entry.id) || entry.id <= 0)) {
      throw new Error("Invalid release listing response.");
    }
    for (const entry of releases) if (entry.tag_name === "stable") {
      if (found !== undefined) throw new Error("Ambiguous stable release listing.");
      found = entry;
    }
    // Finish even after finding stable: a failed later page is uncertainty.
    if (releases.length < 100) return found;
  }
}

function stableFeed(repository) {
  const release = stableRelease(repository);
  if (release === undefined) return undefined;
  if (!Number.isSafeInteger(release?.id) || release.id <= 0) throw new Error("Invalid stable release response.");
  // Only a complete asset listing can confirm that latest.json is absent;
  // every failed page stops publication.
  for (let page = 1; ; page++) {
    const assets = githubJson(repository, `releases/${release.id}/assets?per_page=100&page=${page}`);
    if (!Array.isArray(assets) || assets.length > 100 || assets.some((asset) => typeof asset?.name !== "string")) {
      throw new Error("Invalid stable release assets response.");
    }
    if (assets.some((asset) => asset.name === "latest.json")) {
      const result = run("gh", ["release", "download", "stable", "--repo", repository, "--pattern", "latest.json", "--output", "-"]);
      if (result.status !== 0) throw new Error("Stable feed download failed.");
      return JSON.parse(result.stdout);
    }
    if (assets.length < 100) return undefined;
  }
}

function version(feed) {
  if (typeof feed?.version !== "string") throw new Error("Stable feed has no version string.");
  try { compareVersions(feed.version, feed.version); }
  catch { throw new Error("Stable feed has an invalid version."); }
  return feed.version;
}

function main([kind, file, ...extra]) {
  if (extra.length || kind === "nightly" && file || kind === "stable" && !file || !["stable", "nightly"].includes(kind)) {
    throw new Error("Usage: publication-guard.mjs stable <latest.json> | nightly");
  }
  const repository = process.env.REPO;
  if (!/^[A-Za-z0-9_-][A-Za-z0-9_.-]*\/[A-Za-z0-9_-][A-Za-z0-9_.-]*$/.test(repository ?? "")) {
    throw new Error("Publication guard needs a valid REPO.");
  }
  let publish;
  if (kind === "stable") {
    const candidate = version(JSON.parse(readFileSync(file, "utf8")));
    const current = stableFeed(repository);
    // An equal-version retry must finish asset cleanup and release publication
    // when an earlier attempt uploaded latest.json but failed afterwards.
    publish = current === undefined || compareVersions(candidate, version(current)) >= 0;
  } else {
    const sha = process.env.SHA;
    if (!/^[a-f0-9]{40}$/.test(sha ?? "")) throw new Error("Publication guard needs a valid SHA.");
    const current = nightlyRef(repository);
    publish = current === undefined;
    if (current !== undefined) {
      const result = run("gh", ["api", `repos/${repository}/compare/${current}...${sha}`, "--jq", ".status"]);
      const relation = result.stdout.trim();
      if (result.status !== 0 || !["ahead", "behind", "identical", "diverged"].includes(relation)) {
        throw new Error("Nightly commit comparison failed.");
      }
      publish = relation === "ahead";
    }
  }
  if (!publish) console.error("::notice::Keeping the already published version.");
  console.log(publish ? "publish" : "keep");
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try { main(process.argv.slice(2)); }
  catch { console.error("::error::Publication guard failed; no downloads will be replaced."); process.exitCode = 1; }
}
