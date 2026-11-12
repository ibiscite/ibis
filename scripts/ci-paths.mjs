import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const maxFiles = 3000; // GitHub's pull-request files endpoint stops here.
const full = { core: true, interface: true, native: true, scripts: true, web: true };
const validPath = (path) => typeof path === "string" && path.length > 0 && !/[\x00-\x1f\x7f\\]/.test(path)
  && path.split("/").every((part) => part && part !== "." && part !== "..");
// Markdown beside code or vendored assets can be pinned by Rust tests.
// Only known documentation locations may bypass them.
const documentation = (path) => !path.split("/").includes("vendor") && (
  // docs/cli.md holds options generated from the command line, checked natively.
  (path.endsWith(".md") && path !== "docs/cli.md" && (!path.includes("/") || path.startsWith("docs/") || path === "website/README.md" || path === "apps/desktop/src/ui/README.md"))
  || path === "LICENSE" || path.startsWith(".github/ISSUE_TEMPLATE/")
  // Pull request CI runs no other workflow; actionlint, which always runs, checks them.
  || (/^\.github\/workflows\/[^/]+\.yml$/.test(path) && path !== ".github/workflows/ci.yml")
);
// Nix packaging is built by nix.yml; it cannot affect the app's native checks.
const nixFile = (path) => path === "flake.nix" || path === "flake.lock" || path.startsWith("nix/");
// Generated bindings are also a native contract, checked by `node scripts/gen.mjs --check`.
// The design canvas and its gallery are built and pictured only by the interface checks.
const interfaceFile = (path) => path !== "apps/desktop/src/bindings.ts" && (
  /^apps\/desktop\/src\/.*\.(?:ts|tsx|css|svg)$/.test(path) || /^apps\/desktop\/(?:index|notice|settings)\.html$/.test(path)
  || /^apps\/desktop\/(?:canvas|gallery)\//.test(path) || path === "apps/desktop/vite.canvas.config.ts"
);
// The website is checked by the Web job, except for CLI examples whose exact
// outputs the crates verify against fixtures/website/sample.bib.
const websiteFile = (path) => path.startsWith("website/")
  && path !== "website/content/terminal.txt" && path !== "website/content/writing-apps.json";
// Node scripts and the shell scripts their tests run. No native job runs them,
// and no Rust reads them, so their tests on Linux, Windows and nightly macOS
// are enough. Python, PowerShell and C helpers run in the
// native jobs; CI's own scripts and check.mjs decide what runs, and gen.mjs makes
// files the native job checks, so they run everything.
const scriptFile = (path) => /^scripts\/[^/]+\.(?:mjs|sh)$/.test(path) && !/^scripts\/(?:ci-|check\.mjs$|gen\.mjs$)/.test(path);
// Plain `cargo test` never builds the app, and no crate reads its files except
// its manifest: the engine boundary test reads the crate graph from cargo metadata.
const appFile = (path) => path.startsWith("apps/desktop/") && path !== "apps/desktop/src-tauri/Cargo.toml";
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

/** PRs and main pushes verify the areas their files affect. Unknown lists run all. */
export function classify(event, expected, files) {
  const paths = pathsOf(expected, files);
  if (!["pull_request", "push"].includes(event) || !paths) return { ...full };
  const web = paths.filter((path) => !documentation(path) && !nixFile(path));
  const native = web.filter((path) => !interfaceFile(path) && !websiteFile(path) && !scriptFile(path));
  const core = native.some((path) => !appFile(path));
  // Script tests on other systems run for script changes and, as before, for any crate change.
  const scripts = web.some(scriptFile) || core;
  return {
    core, interface: web.some((path) => !websiteFile(path) && !scriptFile(path)),
    native: native.length > 0, scripts, web: web.length > 0,
  };
}

/** Core PR suites use Linux plus explicitly affected platform implementations. */
export function coreSystems(event, expected, files) {
  const paths = pathsOf(expected, files);
  const all = ["ubuntu-24.04", "windows-2025", "macos-15"];
  if (!["pull_request", "push"].includes(event) || !paths || paths.some((path) => /(?:^|\/)Cargo\.(toml|lock)$/.test(path) || path === ".github/workflows/ci.yml")) return all;
  return ["ubuntu-24.04",
    ...paths.some((path) => /(?:^|[/_.-])windows(?:[/_.-]|$)/i.test(path)) ? ["windows-2025"] : [],
    ...paths.some((path) => /(?:^|[/_.-])(?:macos|darwin)(?:[/_.-]|$)/i.test(path)) ? ["macos-15"] : [],
  ];
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
  let systems = coreSystems("unknown");
  try {
    // FILES: GitHub's list of a pull request's files, one JSON object per line.
    // DIFF: `git diff --name-status -M -z` between a push's two commits.
    const files = process.env.FILES ? readFileSync(process.env.FILES, "utf8").trim().split("\n").map((line) => JSON.parse(line))
      : process.env.DIFF ? parseNameStatus(readFileSync(process.env.DIFF, "utf8")) : null;
    const expected = process.env.FILES ? Number(process.env.EXPECTED) : files?.length;
    result = classify(process.env.EVENT, expected, files);
    systems = coreSystems(process.env.EVENT, expected, files);
  } catch { /* Unreadable or malformed metadata must never skip a check. */ }
  for (const [key, value] of Object.entries(result)) console.log(`${key}=${value}`);
  console.log(`systems=${JSON.stringify(systems)}`);
}
