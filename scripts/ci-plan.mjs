// Chooses which CI jobs a pull request can affect. Anything this cannot map with
// certainty runs the full suite. Every PR must first have a portable tree.
// Main pushes, schedules, manual runs, the `ci:full` label and the repository
// variable CI_SELECTION=full always run everything. CI result checks that every
// selected job passed and every other one was skipped (decision 0049).
import { spawnSync } from "node:child_process";
import { appendFileSync } from "node:fs";
import { createRequire } from "node:module";
import { isAbsolute, join, resolve, sep } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export const SELECTABLE = ["rust", "workspace", "pictures", "csl"];
const FULL = Object.fromEntries(SELECTABLE.map((job) => [job, true]));

// Build inputs and generated files can affect jobs outside their own folder.
const ALWAYS_FULL = [
  /(^|\/)(Cargo\.(toml|lock)|rust-toolchain(\.toml)?|build\.rs)$/,
  /(^|\/)\.cargo(\/|$)/,
  /^docs\/cli\.md$/,
  /^apps\/desktop\/src\/bindings\.ts$/,
];
// The web job checks links in prose.
const DOCS = [/^docs\/.+\.md$/, /^[^/]+\.md$/];
// The interface: its own tests, the pictures, and the app that embeds it.
const INTERFACE = /^apps\/desktop\/(?!src-tauri(?:\/|$))/;
const ROOT = fileURLToPath(new URL("../", import.meta.url));
const DESKTOP = join(ROOT, "apps", "desktop");

// Win32 rejects device names even with an extension. Check each directory
// component too, including collisions on case-insensitive filesystems.
const WINDOWS_DEVICE = /^(con|prn|aux|nul|com[0-9¹²³]|lpt[0-9¹²³]|conin\$|conout\$)$/i;
const WINDOWS_FORBIDDEN = /[<>:"\\|?*\u0000-\u001f]|[ .]$/u;
const NTFS_GIT_DIR = /^(\.git|git~1)$/i;

function checkPortablePaths(paths) {
  const seen = new Map();
  for (const path of paths) {
    let prefix = "";
    for (const part of path.split("/")) {
      if (WINDOWS_FORBIDDEN.test(part) || WINDOWS_DEVICE.test(part.split(".")[0].trimEnd()) || NTFS_GIT_DIR.test(part)) {
        throw new Error(`Non-portable filename: ${JSON.stringify(path)}`);
      }
      prefix = prefix ? `${prefix}/${part}` : part;
      const canonical = prefix.normalize("NFD").toUpperCase();
      const other = seen.get(canonical);
      if (other && other !== prefix) {
        throw new Error(`Conflicting checkout paths: ${JSON.stringify(other)} and ${JSON.stringify(prefix)}`);
      }
      seen.set(canonical, prefix);
    }
  }
}

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
    csl: affected.has("ibis-csl"),
    full: false,
    reason: affected.size || interfaceChanged ? `affects ${[...[...affected].sort(), ...(interfaceChanged ? ["the interface"] : [])].join(", ")}` : "documentation only",
  };
}

/** Everything runs except on a pull request without the label or the rollback variable. */
export function wantsFull({ event, labels = [], selection = "" }) {
  if (event !== "pull_request") return `a ${event || "unknown"} run`;
  if (selection === "full") return "CI_SELECTION is full";
  if (!Array.isArray(labels) || labels.some((label) => typeof label !== "string")) return "the labels could not be read";
  if (labels.includes("ci:full")) return "the ci:full label";
  return "";
}

function git(...args) {
  const result = spawnSync("git", args, { timeout: 60_000, maxBuffer: 16 * 1024 * 1024 });
  if (result.status !== 0) throw new Error(`git ${args[0]} failed`);
  return new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(result.stdout);
}

function treePaths() {
  return git("ls-tree", "-r", "--name-only", "-z", "HEAD").split("\0").filter(Boolean);
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

/** Resolve inputs with the Picture gate's Vite configuration, without writing a bundle. */
export async function canvasInputs(paths) {
  const require = createRequire(join(DESKTOP, "package.json"));
  const { build } = await import(pathToFileURL(require.resolve("vite")).href);
  const inputs = new Set();
  let publicDir;
  await build({
    root: DESKTOP,
    configFile: join(DESKTOP, "vite.canvas.config.ts"),
    logLevel: "silent",
    build: { write: false },
    plugins: [{
      name: "ci-canvas-inputs",
      configResolved(config) {
        publicDir = config.publicDir && resolve(config.publicDir);
        for (const file of config.configFileDependencies) inputs.add(resolve(file));
      },
      generateBundle(_, bundle) {
        for (const id of this.getModuleIds()) {
          const file = id.split("?")[0];
          if (isAbsolute(file)) inputs.add(resolve(file));
        }
        for (const output of Object.values(bundle)) {
          if (output.type !== "asset") continue;
          for (const file of output.originalFileNames) inputs.add(resolve(DESKTOP, file));
        }
      },
    }],
  });
  if (!inputs.size) throw new Error("the canvas build reported no inputs");
  return new Set(paths.filter((path) => {
    const file = resolve(ROOT, path);
    return inputs.has(file) || (publicDir && file.startsWith(`${publicDir}${sep}`));
  }));
}

async function pictureInputs(paths) {
  const install = spawnSync("pnpm", ["-C", DESKTOP, "install", "--frozen-lockfile"], { timeout: 120_000 });
  if (install.status !== 0) throw new Error("installing canvas build dependencies failed");
  return canvasInputs(paths);
}

export async function decide(env, read = { changedPaths, treePaths, workspaceCrates, pictureInputs }) {
  // This is a required check, even when selection is disabled or a diff is unreadable.
  const tracked = env.EVENT === "pull_request" ? new Set(read.treePaths()) : new Set();
  checkPortablePaths(tracked);
  let labels = [];
  try { labels = JSON.parse(env.LABELS || "[]"); } catch { return { ...FULL, full: true, reason: "the labels could not be read" }; }
  const why = wantsFull({ event: env.EVENT, labels, selection: env.SELECTION });
  if (why) return { ...FULL, full: true, reason: why };
  try {
    const paths = read.changedPaths();
    const chosen = plan(paths, read.workspaceCrates());
    if (chosen.pictures) return chosen;
    // A deletion can remove a member of a glob from the new build's graph.
    const removed = paths.find((path) => !tracked.has(path));
    if (removed) return { ...chosen, pictures: true, reason: `${chosen.reason}; ${removed} was removed` };
    const input = (await read.pictureInputs(paths)).values().next().value;
    return input ? { ...chosen, pictures: true, reason: `${chosen.reason}; ${input} is a canvas build input` } : chosen;
  } catch (error) {
    return { ...FULL, full: true, reason: `${error.message}` };
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const chosen = await decide(process.env);
  const line = `${chosen.full ? "Full suite" : "Selected"}: ${chosen.reason}. ${SELECTABLE.map((job) => `${job}=${chosen[job]}`).join(" ")}`;
  console.log(line);
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${line}\n`);
  appendFileSync(process.env.GITHUB_OUTPUT, SELECTABLE.map((job) => `${job}=${chosen[job]}\n`).join(""));
}
