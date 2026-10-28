import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const maxFiles = 3000; // GitHub's pull-request files endpoint stops here.
const full = { native: true, web: true };
const validPath = (path) => typeof path === "string" && path.length > 0 && !/[\x00-\x1f\x7f\\]/.test(path)
  && path.split("/").every((part) => part && part !== "." && part !== "..");
// Markdown beside code or vendored assets can be pinned by Rust tests.
// Only known documentation locations may bypass them.
const documentation = (path) => !path.split("/").includes("vendor") && (
  (path.endsWith(".md") && (!path.includes("/") || path.startsWith("docs/") || path === "website/README.md"))
  || path === "LICENSE" || path.startsWith(".github/ISSUE_TEMPLATE/")
);
const interfaceFile = (path) => /^apps\/desktop\/src\/.*\.(?:ts|tsx|css|svg)$/.test(path)
  || path === "apps/desktop/index.html";
// The website is plain files checked by the script tests in the Web job.
const websiteFile = (path) => path.startsWith("website/");

export function classify(event, expected, files) {
  if (event !== "pull_request") return { ...full };
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
  return { native: paths.some((path) => !documentation(path) && !interfaceFile(path) && !websiteFile(path)), web: paths.some((path) => !documentation(path)) };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  let result = full;
  try {
    const files = process.env.FILES ? readFileSync(process.env.FILES, "utf8").trim().split("\n").map((line) => JSON.parse(line)) : null;
    result = classify(process.env.EVENT, Number(process.env.EXPECTED), files);
  } catch { /* Unreadable or malformed metadata must never skip a check. */ }
  for (const [key, value] of Object.entries(result)) console.log(`${key}=${value}`);
}
