// A named large file may shrink, but its exception can never grow.
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const baselinePath = "scripts/file-size-baseline.json";
const normalized = (name) => name.replaceAll("\\", "/");
export const sourceFile = (name) => /\.(?:rs|[cm]?[jt]sx?|py|css|sh|ps1|html|nix|swift|c|h)$/.test(normalized(name))
  && !/(?:^|\/)(?:vendor|node_modules|target|dist|gen)\//.test(normalized(name))
  && !normalized(name).startsWith("brand/exports/") && normalized(name) !== "apps/desktop/src/bindings.ts";
export const lineCount = (text) => text === "" ? 0 : text.split("\n").length - Number(text.endsWith("\n"));

export function sizeErrors(files, baseline, previous = null) {
  const errors = [];
  const counts = Object.fromEntries(Object.entries(files).filter(([name]) => sourceFile(name)).map(([name, text]) => [normalized(name), lineCount(text)]));
  for (const [name, count] of Object.entries(counts)) {
    const limit = baseline[name] ?? 600;
    if (!Number.isSafeInteger(limit) || limit < 600 || count > limit) errors.push(`${name}: ${count} lines; split the file to fit ${limit} lines (new files: 600).`);
  }
  for (const [name, limit] of Object.entries(baseline)) {
    if (!sourceFile(name) || !Number.isSafeInteger(limit) || limit <= 600 || counts[name] !== limit) errors.push(`${name}: lower the baseline to ${counts[name] > 600 ? counts[name] : "remove this entry"}; it must match the current count above 600.`);
    if (previous !== null && (!Object.hasOwn(previous, name) || limit > previous[name])) errors.push(`${name}: baseline exceptions cannot be added or grow; split the file to 600 lines or its previous limit.`);
  }
  return errors;
}

function git(args, allowFailure = false) {
  const result = spawnSync("git", ["--literal-pathspecs", "-c", "core.fsmonitor=false", ...args], { encoding: "utf8", timeout: 30000, killSignal: "SIGKILL", maxBuffer: 32 * 1024 * 1024 });
  if (result.status !== 0 || result.error) {
    if (allowFailure) return null;
    throw new Error("File-size check cannot read Git metadata; fetch origin/main and retry.");
  }
  return result.stdout;
}

export function previousBaseline() {
  const event = process.env.GITHUB_EVENT_PATH ? JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, "utf8")) : {};
  const sha = event.pull_request?.base?.sha ?? event.merge_group?.base_sha ?? event.before;
  if (sha != null && !/^[a-f0-9]{40}$/.test(sha)) throw new Error("File-size check needs a valid base SHA; rerun CI.");
  const reference = sha ?? "origin/main";
  if (git(["cat-file", "-e", `${reference}^{commit}`], true) === null) {
    if (!sha) throw new Error("File-size check needs origin/main; run git fetch origin main.");
    git(["fetch", "--no-tags", "--depth=1", "origin", sha]);
  }
  if (!git(["ls-tree", "--name-only", reference, "--", baselinePath]).trim()) return null; // Initial baseline only.
  return JSON.parse(git(["show", `${reference}:${baselinePath}`]));
}

function main() {
  if (process.argv.slice(2).some((arg) => arg !== "--write-baseline")) throw new Error("Usage: node scripts/check-file-size.mjs [--write-baseline]");
  const files = Object.fromEntries(git(["ls-files", "-z"]).split("\0").filter((name) => sourceFile(name) && existsSync(name)).map((name) => [name, readFileSync(name, "utf8")]));
  const write = process.argv.includes("--write-baseline");
  const baseline = write ? Object.fromEntries(Object.entries(files).map(([name, text]) => [name, lineCount(text)]).filter(([, n]) => n > 600).sort(([a], [b]) => a.localeCompare(b, "en"))) : JSON.parse(readFileSync(baselinePath, "utf8"));
  const errors = sizeErrors(files, baseline, previousBaseline());
  if (errors.length) {
    console.error(errors.join("\n") + "\nAfter shrinking files, run node scripts/check-file-size.mjs --write-baseline and commit the lowered baseline.");
    process.exitCode = 1;
  } else {
    if (write) writeFileSync(baselinePath, JSON.stringify(baseline, null, 2) + "\n");
    console.log("File-size ratchet passed.");
  }
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try { main(); } catch (error) { console.error(error.message); process.exitCode = 1; }
}
