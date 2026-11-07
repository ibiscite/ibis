import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const maxFiles = 3000; // GitHub's pull-request files endpoint stops here.
const full = { core: true, interface: true, native: true, proven: false, scripts: true, web: true };
const none = { core: false, interface: false, native: false, proven: true, scripts: false, web: false };
const validPath = (path) => typeof path === "string" && path.length > 0 && !/[\x00-\x1f\x7f\\]/.test(path)
  && path.split("/").every((part) => part && part !== "." && part !== "..");
// Markdown beside code or vendored assets can be pinned by Rust tests.
// Only known documentation locations may bypass them.
const documentation = (path) => !path.split("/").includes("vendor") && (
  (path.endsWith(".md") && (!path.includes("/") || path.startsWith("docs/") || path === "website/README.md" || path === "apps/desktop/src/ui/README.md"))
  || path === "LICENSE" || path.startsWith(".github/ISSUE_TEMPLATE/")
);
// Generated bindings are also a native contract, checked by Rust tests.
const interfaceFile = (path) => path !== "apps/desktop/src/bindings.ts" && (
  /^apps\/desktop\/src\/.*\.(?:ts|tsx|css|svg)$/.test(path) || path === "apps/desktop/index.html"
);
// The website is plain files checked by the script tests in the Web job.
const websiteFile = (path) => path.startsWith("website/");
// Node scripts and the shell scripts their tests run. No native job runs them,
// and no Rust reads them, so their tests on Linux, Windows and (outside pull
// requests) macOS are enough. Python, PowerShell and C helpers run in the
// native jobs; CI's own scripts and check.mjs decide what runs, so they run everything.
const scriptFile = (path) => /^scripts\/[^/]+\.(?:mjs|sh)$/.test(path) && !/^scripts\/(?:ci-|check\.mjs$)/.test(path);
// Plain `cargo test` never builds the app, and no crate reads its files except
// its manifest: the engine boundary test reads the crate graph from cargo metadata.
const appFile = (path) => path.startsWith("apps/desktop/") && path !== "apps/desktop/src-tauri/Cargo.toml";
// What main's caches are keyed on: the Rust and pnpm caches change only with these.
const cacheInput = (path) => /(?:^|\/)Cargo\.(?:toml|lock)$/.test(path) || /^rust-toolchain(?:\.toml)?$/.test(path)
  || path.startsWith(".cargo/") || /^apps\/desktop\/(?:package\.json|pnpm-lock\.yaml)$/.test(path) || path === ".github/workflows/ci.yml";

/** Every path a file list names, or null when the list is unclear. */
function pathsOf(expected, files) {
  if (!Number.isInteger(expected) || expected < 1 || expected > maxFiles || !Array.isArray(files) || files.length !== expected) return null;
  const paths = [];
  const names = new Set();
  for (const file of files) {
    if (!file || !["added", "removed", "modified", "renamed", "copied", "changed", "unchanged"].includes(file.status) || !validPath(file.filename) || names.has(file.filename)
      || (file.status === "renamed" && !validPath(file.previous_filename))
      || (file.previous_filename != null && !validPath(file.previous_filename))) return null;
    names.add(file.filename);
    paths.push(file.filename);
    if (file.previous_filename != null) paths.push(file.previous_filename);
  }
  return paths;
}

/**
 * Which checks a run needs. A pull request and a merge queue entry run what
 * their files can affect. `proven` means a passing run already checked this
 * exact tree: the pull request's own run for a queue entry that changes nothing
 * beside it, or the queue's run for the commit main now points at. A queue
 * entry reuses its pull request's run only when that run skipped nothing the
 * queue must add: pull requests leave the crates' and scripts' macOS tests to
 * the queue. Main still runs everything when its caches' inputs changed, so it
 * saves new caches.
 */
export function classify(event, expected, files, proven = false) {
  const paths = pathsOf(expected, files);
  if (proven === true && event === "push" && paths && !paths.some(cacheInput)) return { ...none };
  if (!["pull_request", "merge_group"].includes(event) || !paths) return { ...full };
  const web = paths.filter((path) => !documentation(path));
  const native = web.filter((path) => !interfaceFile(path) && !websiteFile(path) && !scriptFile(path));
  const core = native.some((path) => !appFile(path));
  // Script tests on other systems run for script changes and, as before, for any crate change.
  const scripts = web.some(scriptFile) || core;
  if (proven === true && event === "merge_group" && !scripts) return { ...none };
  return {
    core, interface: web.some((path) => !websiteFile(path) && !scriptFile(path)),
    native: native.length > 0, proven: false, scripts, web: web.length > 0,
  };
}

/** `git diff --name-status -M -z` output in the shape of GitHub's file list,
 * or null when it is unreadable. */
export function parseNameStatus(output) {
  if (typeof output !== "string") return null;
  const statuses = { A: "added", C: "copied", D: "removed", M: "modified", R: "renamed", T: "changed" };
  const fields = output.split("\0").filter(Boolean);
  const files = [];
  while (fields.length > 0) {
    const status = fields.shift();
    const moved = /^[RC]/.test(status);
    const previous_filename = moved ? fields.shift() : null;
    files.push({ filename: fields.shift(), previous_filename, status: statuses[status[0]] ?? "unknown" });
  }
  return files;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  let result = full;
  try {
    // FILES: GitHub's list of a pull request's files, one JSON object per line.
    // DIFF: `git diff --name-status -M -z` between a queue entry's or a push's two commits.
    const files = process.env.FILES ? readFileSync(process.env.FILES, "utf8").trim().split("\n").map((line) => JSON.parse(line))
      : process.env.DIFF ? parseNameStatus(readFileSync(process.env.DIFF, "utf8")) : null;
    const expected = process.env.FILES ? Number(process.env.EXPECTED) : files?.length;
    result = classify(process.env.EVENT, expected, files, process.env.PROVEN === "true");
  } catch { /* Unreadable or malformed metadata must never skip a check. */ }
  for (const [key, value] of Object.entries(result)) console.log(`${key}=${value}`);
}
