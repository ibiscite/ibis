// Chooses which CI jobs a pull request can affect. Anything this cannot map with
// certainty runs the full suite: the plan only ever widens, it never fails a run.
// Main pushes, schedules, manual runs, the `ci:full` label and the repository
// variable CI_SELECTION=full always run everything. CI result checks that every
// selected job passed and every other one was skipped (decision 0049).
import { spawnSync } from "node:child_process";
import { appendFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

export const SELECTABLE = ["rust", "workspace", "pictures"];
const FULL = Object.fromEntries(SELECTABLE.map((job) => [job, true]));

// Inside a mapped folder, but read by more than that folder's own crate or job.
const ALWAYS_FULL = [/(^|\/)build\.rs$/, /^docs\/cli\.md$/, /^apps\/desktop\/src\/bindings\.ts$/];
// Prose nothing compiles or renders; the web job, which always runs, checks its links.
const DOCS = [/^docs\/.+\.md$/, /^[^/]+\.md$/];
// The interface: its own tests, the pictures, and the app that embeds it.
const INTERFACE = /^apps\/desktop\/(?!src-tauri\/)/;

/** Workspace crates by folder, with their workspace dependencies, from `cargo metadata --no-deps`. */
export function crates(metadata) {
  const root = metadata.workspace_root.replaceAll("\\", "/").replace(/\/$/, "");
  const defaults = new Set(metadata.workspace_default_members);
  return metadata.packages.map((pkg) => ({
    name: pkg.name,
    dir: pkg.manifest_path.replaceAll("\\", "/").slice(root.length + 1).replace(/\/?Cargo\.toml$/, ""),
    deps: pkg.dependencies.filter((dep) => dep.path).map((dep) => dep.name),
    workspace: defaults.has(pkg.id),
  }));
}

/** The plan for a pull request's changed paths; `reason` says why it ran everything. */
export function plan(paths, workspace) {
  if (!Array.isArray(paths) || paths.length === 0) return { ...FULL, full: true, reason: "no changed paths could be read" };
  const byDepth = [...workspace].filter((crate) => crate.dir).sort((a, b) => b.dir.length - a.dir.length);
  const changed = new Set();
  let interfaceChanged = false;
  for (const path of paths) {
    if (typeof path !== "string" || path === "" || ALWAYS_FULL.some((rule) => rule.test(path))) return { ...FULL, full: true, reason: `${path} affects more than one part` };
    if (DOCS.some((rule) => rule.test(path))) continue;
    if (INTERFACE.test(path)) { interfaceChanged = true; continue; }
    const owner = byDepth.find((crate) => path.startsWith(`${crate.dir}/`));
    if (!owner) return { ...FULL, full: true, reason: `${path} is not mapped to a job` };
    changed.add(owner.name);
  }
  // Every crate that depends on a changed one, directly or not, is affected too.
  const affected = new Set(changed);
  for (let grew = true; grew;) {
    grew = false;
    for (const crate of workspace) {
      if (!affected.has(crate.name) && crate.deps.some((dep) => affected.has(dep))) { affected.add(crate.name); grew = true; }
    }
  }
  const tested = workspace.filter((crate) => affected.has(crate.name));
  if (tested.some((crate) => crate.name === "ibis-fuzz") && tested.every((crate) => crate.name === "ibis-fuzz")) {
    return { ...FULL, full: true, reason: "only the fuzz crate changed, which no pull request job tests" };
  }
  const workspaceAffected = tested.some((crate) => crate.workspace);
  return {
    rust: workspaceAffected || tested.some((crate) => crate.name !== "ibis-fuzz") || interfaceChanged,
    workspace: workspaceAffected,
    pictures: interfaceChanged,
    full: false,
    reason: affected.size || interfaceChanged ? `affects ${[...[...affected].sort(), ...(interfaceChanged ? ["the interface"] : [])].join(", ")}` : "documentation only",
  };
}

/** Everything runs except on a pull request without the label or the rollback variable. */
export function wantsFull({ event, labels = [], selection = "" }) {
  if (event !== "pull_request") return `a ${event || "unknown"} run`;
  if (selection === "full") return "CI_SELECTION is full";
  if (labels.includes("ci:full")) return "the ci:full label";
  return "";
}

function git(...args) {
  const result = spawnSync("git", args, { encoding: "utf8", timeout: 60_000, maxBuffer: 16 * 1024 * 1024 });
  if (result.status !== 0) throw new Error(`git ${args[0]} failed`);
  return result.stdout;
}

/** Paths a pull request changes: its merge commit against the base it merges into. */
function changedPaths() {
  const parents = git("rev-list", "--parents", "-n", "1", "HEAD").trim().split(" ");
  if (parents.length !== 3) throw new Error("the checkout is not the pull request's merge commit");
  return git("diff", "--name-only", "--no-renames", "-z", "HEAD^1", "HEAD").split("\0").filter(Boolean);
}

function workspaceCrates() {
  const result = spawnSync("cargo", ["metadata", "--format-version", "1", "--no-deps", "--locked"], { encoding: "utf8", timeout: 120_000, maxBuffer: 64 * 1024 * 1024 });
  if (result.status !== 0) throw new Error("cargo metadata failed");
  return crates(JSON.parse(result.stdout));
}

export function decide(env, read = { changedPaths, workspaceCrates }) {
  let labels = [];
  try { labels = JSON.parse(env.LABELS || "[]"); } catch { return { ...FULL, full: true, reason: "the labels could not be read" }; }
  const why = wantsFull({ event: env.EVENT, labels, selection: env.SELECTION });
  if (why) return { ...FULL, full: true, reason: why };
  try {
    return plan(read.changedPaths(), read.workspaceCrates());
  } catch (error) {
    return { ...FULL, full: true, reason: `${error.message}` };
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const chosen = decide(process.env);
  const line = `${chosen.full ? "Full suite" : "Selected"}: ${chosen.reason}. ${SELECTABLE.map((job) => `${job}=${chosen[job]}`).join(" ")}`;
  console.log(line);
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${line}\n`);
  appendFileSync(process.env.GITHUB_OUTPUT, SELECTABLE.map((job) => `${job}=${chosen[job]}\n`).join(""));
}
