import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const maxFiles = 3000; // GitHub's pull-request files endpoint stops here.
const full = { native: true, web: true, nix: true };
const validPath = (path) => typeof path === "string" && path.length > 0 && !/[\x00-\x1f\x7f\\]/.test(path)
  && path.split("/").every((part) => part && part !== "." && part !== "..");
const documentation = (path) => !path.startsWith("fixtures/") && (path.endsWith(".md") || path.startsWith("LICENSE") || path.startsWith(".github/ISSUE_TEMPLATE/"));
const interfaceFile = (path) => /^apps\/desktop\/src\/.*\.(?:ts|tsx|css|svg)$/.test(path)
  || ["apps/desktop/index.html", "apps/desktop/gallery.html", "scripts/gallery-contact-sheet.mjs", "scripts/gallery-geometry.mjs", "scripts/gallery-geometry.test.mjs"].includes(path);
const nixInput = (path) => ["flake.nix", "flake.lock", "Cargo.lock", "apps/desktop/package.json", "apps/desktop/pnpm-lock.yaml"].includes(path) || path.startsWith("nix/");

export function classify(event, expected, files) {
  if (event !== "pull_request") return { native: event !== "schedule", web: event !== "schedule", nix: ["schedule", "workflow_dispatch"].includes(event) };
  if (!Number.isInteger(expected) || expected < 1 || expected > maxFiles || !Array.isArray(files) || files.length !== expected) return { ...full };
  const paths = [];
  const names = new Set();
  for (const file of files) {
    if (!file || !["added", "removed", "modified", "renamed", "copied", "changed", "unchanged"].includes(file.status) || !validPath(file.filename) || names.has(file.filename)
      || (file.status === "renamed" && !validPath(file.previous_filename))
      || (file.previous_filename != null && !validPath(file.previous_filename))) return { ...full };
    names.add(file.filename);
    paths.push(file.filename);
    if (file.previous_filename != null) paths.push(file.previous_filename);
  }
  return { native: paths.some((path) => !documentation(path) && !interfaceFile(path)), web: paths.some((path) => !documentation(path)), nix: paths.some(nixInput) };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  let result = full;
  try {
    const files = process.env.FILES ? readFileSync(process.env.FILES, "utf8").trim().split("\n").map((line) => JSON.parse(line)) : null;
    result = classify(process.env.EVENT, Number(process.env.EXPECTED), files);
  } catch { /* Unreadable or malformed metadata must never skip a check. */ }
  for (const [key, value] of Object.entries(result)) console.log(`${key}=${value}`);
}
