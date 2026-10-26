// A named large file may shrink, but its exception can never grow, and no
// file may grow past its size on main. Comparing with main (not only the
// committed baseline) lets a shrink land without editing the shared baseline,
// so concurrent pull requests in the merge queue never collide on it.
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const baselinePath = "scripts/file-size-baseline.json";
const normalized = (name) => name.replaceAll("\\", "/");
export const sourceFile = (name) => /\.(?:rs|[cm]?[jt]sx?|py|css|sh|ps1|html|nix|swift|c|h)$/.test(normalized(name))
  && !/(?:^|\/)(?:vendor|node_modules|target|dist|gen)\//.test(normalized(name))
  && !normalized(name).startsWith("brand/exports/") && normalized(name) !== "apps/desktop/src/bindings.ts";
export const lineCount = (text) => text === "" ? 0 : text.split("\n").length - Number(text.endsWith("\n"));

// base: line counts on main for files above 600 lines (absent = new file), or null.
export function sizeErrors(files, baseline, previous = null, base = null) {
  const errors = [];
  const names = new Set();
  for (const name of Object.keys(files).filter(sourceFile)) {
    const key = normalized(name);
    if (names.has(key)) errors.push(`${name}: normalized path collision; rename one file so each source has a distinct path.`);
    names.add(key);
  }
  const counts = Object.fromEntries(Object.entries(files).filter(([name]) => sourceFile(name)).map(([name, text]) => [normalized(name), lineCount(text)]));
  for (const [name, count] of Object.entries(counts)) {
    const cap = baseline[name] ?? 600;
    const limit = base === null ? cap : Math.min(cap, Math.max(600, base[name] ?? 600));
    if (!Number.isSafeInteger(limit) || limit < 600 || count > limit) errors.push(`${name}: ${count} lines; split the file to fit ${limit} lines (its size on main, its baseline or 600 for new files).`);
  }
  for (const [name, limit] of Object.entries(baseline)) {
    if (!sourceFile(name) || !Number.isSafeInteger(limit) || limit <= 600) errors.push(`${name}: baseline entries must be source files above 600 lines; remove this entry.`);
    if (previous !== null && (!Object.hasOwn(previous, name) || limit > previous[name])) errors.push(`${name}: baseline exceptions cannot be added or grow; split the file to 600 lines or its previous limit.`);
  }
  return errors;
}

function git(args, allowFailure = false) {
  const result = spawnSync("git", ["--literal-pathspecs", "-c", "core.fsmonitor=false", ...args], { encoding: "utf8", timeout: 30000, killSignal: "SIGKILL", maxBuffer: 32 * 1024 * 1024 });
  if (result.status !== 0 || result.error) {
    if (allowFailure) return null;
    const detail = result.stderr?.trim() || result.error?.message || `Git exited with status ${result.status}.`;
    throw new Error(`File-size check cannot read Git metadata. Fix the Git error below, then rerun the check:\n${detail}`);
  }
  return result.stdout;
}

function commit(sha) {
  if (sha != null && !/^[a-f0-9]{40}$/.test(sha)) throw new Error("File-size check needs a valid base SHA; rerun CI.");
  const reference = sha ?? "origin/main";
  if (git(["cat-file", "-e", `${reference}^{commit}`], true) === null) {
    if (!sha) throw new Error("File-size check needs origin/main; run git fetch origin main.");
    git(["fetch", "--no-tags", "--depth=1", "origin", sha]);
  }
  return reference;
}

// The main commit to compare with: the one the checked tree was built on.
// A pull request checks out GitHub's merge of its head into main, whose first
// parent can be newer than the event's base SHA; the queue's base and the
// previous push are exact. The merge queue checks every change again against
// the real main, so a stale pull request base never lands an older limit.
export function baseReference() {
  const event = process.env.GITHUB_EVENT_PATH ? JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, "utf8")) : {};
  if (event.pull_request) {
    // Read the parents from the commit itself: a shallow checkout hides them from rev-list.
    const [first, second] = [...git(["cat-file", "-p", "HEAD"]).split("\n\n")[0].matchAll(/^parent ([a-f0-9]{40})$/gm)].map((match) => match[1]);
    return commit(second && second === event.pull_request.head?.sha ? first : event.pull_request.base?.sha);
  }
  return commit(event.merge_group?.base_sha ?? event.before);
}

export function previousBaseline(reference) {
  if (!git(["ls-tree", "--name-only", reference, "--", baselinePath]).trim()) return null; // Initial baseline only.
  return JSON.parse(git(["show", `${reference}:${baselinePath}`]));
}

// Only files above 600 lines can exceed a limit, so only they are read on main.
function baseCounts(reference, files) {
  return Object.fromEntries(Object.entries(files).filter(([, text]) => lineCount(text) > 600).flatMap(([name]) => {
    // A missing file is new; any other Git failure stops the check.
    if (!git(["ls-tree", "--name-only", reference, "--", normalized(name)]).trim()) return [];
    return [[normalized(name), lineCount(git(["cat-file", "blob", `${reference}:${normalized(name)}`]))]];
  }));
}

function main() {
  if (process.argv.slice(2).some((arg) => arg !== "--write-baseline")) throw new Error("Usage: node scripts/check-file-size.mjs [--write-baseline]");
  const files = Object.fromEntries(git(["ls-files", "-z"]).split("\0").filter((name) => sourceFile(name) && existsSync(name)).map((name) => [name, readFileSync(name, "utf8")]));
  const write = process.argv.includes("--write-baseline");
  const baseline = write ? Object.fromEntries(Object.entries(files).map(([name, text]) => [name, lineCount(text)]).filter(([, n]) => n > 600).sort(([a], [b]) => a.localeCompare(b, "en"))) : JSON.parse(readFileSync(baselinePath, "utf8"));
  const base = baseReference();
  const errors = sizeErrors(files, baseline, previousBaseline(base), baseCounts(base, files));
  if (errors.length) {
    console.error(errors.join("\n") + "\nSplit the files named above. A shrink needs no baseline edit.");
    process.exitCode = 1;
  } else {
    if (write) writeFileSync(baselinePath, JSON.stringify(baseline, null, 2) + "\n");
    console.log("File-size ratchet passed.");
  }
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try { main(); } catch (error) { console.error(error.message); process.exitCode = 1; }
}
