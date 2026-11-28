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
const DOCS = [/^docs\/.+\.md$/, /^[^/]+\.md$/, /^\.github\/pull_request_template\.md$/, /^\.(agents|claude)\/skills(\/|$)/];
// The interface: its own tests, the pictures, and the app that embeds it.
const INTERFACE = /^apps\/desktop\/(?!src-tauri(?:\/|$))/;
const ROOT = fileURLToPath(new URL("../", import.meta.url));
const DESKTOP = join(ROOT, "apps", "desktop");

// The default actions/checkout paths on the hosted CI matrix. Include the
// trailing separator. Path limits are exclusive (the terminating NUL must fit).
const CHECKOUT_LIMITS = {
  "ubuntu-24.04": {
    root: "/home/runner/work/ibis/ibis/",
    measure: Buffer.byteLength,
    unit: "UTF-8 bytes",
    componentMax: 255,
    pathLimit: 4096,
  },
  "macos-15": {
    root: "/Users/runner/work/ibis/ibis/",
    measure: Buffer.byteLength,
    unit: "UTF-8 bytes",
    componentMax: 255,
    pathLimit: 1024,
    // APFS compares canonical names. Include ignored characters to catch Git's HFS aliases.
    fold: (name) => name.normalize("NFD").replace(/\p{Default_Ignorable_Code_Point}/gu, "").toLowerCase().toUpperCase().normalize("NFD"),
    reserved: /^\.GIT$/,
  },
  "windows-2025": {
    root: "D:\\a\\ibis\\ibis\\",
    measure: (name) => name.length,
    unit: "UTF-16 code units",
    componentMax: 255,
    pathLimit: 260,
    // Git for Windows uses 248 for mkdir without core.longpaths.
    directoryLimit: 248,
    fold: (name) => name.toLowerCase().toUpperCase(),
    forbidden: /[<>:"\\|?*\u0000-\u001f]|[ .]$/u,
    // Win32 rejects device names even with an extension.
    reserved: /^(?:GIT~1$|(?:CON|PRN|AUX|NUL|COM[0-9¹²³]|LPT[0-9¹²³]|CONIN\$|CONOUT\$) *(?:\.|$))/u,
  },
};

export function checkPortablePaths(paths, system) {
  const { root, measure, unit, componentMax, pathLimit, directoryLimit, fold, forbidden, reserved } = CHECKOUT_LIMITS[system];
  const rootLength = measure(root);
  const seen = new Map();
  for (const path of paths) {
    const parts = path.split("/");
    let prefix = "";
    let canonical = "";
    for (const [index, part] of parts.entries()) {
      const name = fold ? fold(part) : part;
      if (!name || name === "." || name === ".." || /^\.git$/i.test(part) || forbidden?.test(part) || reserved?.test(name)) {
        throw new Error(`Non-portable filename on ${system}: ${JSON.stringify(path)}`);
      }
      if (measure(part) > componentMax) {
        throw new Error(`${system} path component exceeds ${componentMax} ${unit}: ${JSON.stringify(part)} in ${JSON.stringify(path)}`);
      }
      prefix = prefix ? `${prefix}/${part}` : part;
      if (directoryLimit && index < parts.length - 1 && rootLength + measure(prefix) >= directoryLimit) {
        throw new Error(`${system} checkout directory exceeds ${directoryLimit - 1} ${unit} including the checkout root: ${JSON.stringify(prefix)}`);
      }
      if (!fold) continue;
      canonical += index ? `/${name}` : name;
      const other = seen.get(canonical);
      if (other !== undefined && other !== prefix) {
        throw new Error(`Conflicting checkout paths on ${system}: ${JSON.stringify(other)} and ${JSON.stringify(prefix)}`);
      }
      seen.set(canonical, prefix);
    }
    if (rootLength + measure(path) >= pathLimit) {
      throw new Error(`${system} checkout path exceeds ${pathLimit - 1} ${unit} including the checkout root: ${JSON.stringify(path)}`);
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
export function wantsFull({ event, labels = [], selection = "", macosRunnerOverride = "" }) {
  if (event !== "pull_request") return `a ${event || "unknown"} run`;
  if (selection === "full") return "CI_SELECTION is full";
  if (macosRunnerOverride === "true") return "the macOS runner has a custom checkout root";
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

/** Paths this branch changes against origin/main, committed or not, for `--local`. */
function localPaths() {
  const base = git("merge-base", "origin/main", "HEAD").trim();
  const lists = [
    git("diff", "--name-only", "--no-renames", "-z", base),
    git("ls-files", "--others", "--exclude-standard", "-z"),
  ];
  return [...new Set(lists.join("\0").split("\0").filter(Boolean))];
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
  if (env.EVENT === "pull_request") {
    for (const system of Object.keys(CHECKOUT_LIMITS)) checkPortablePaths(tracked, system);
  }
  let labels = [];
  try { labels = JSON.parse(env.LABELS || "[]"); } catch { return { ...FULL, full: true, reason: "the labels could not be read" }; }
  const why = wantsFull({ event: env.EVENT, labels, selection: env.SELECTION, macosRunnerOverride: env.MACOS_RUNNER_OVERRIDE });
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

if (process.argv[1] === fileURLToPath(import.meta.url) && process.argv[2] === "--local") {
  // What scripts/check.sh runs: the same plan for this branch, without the
  // picture inputs, since the picture gate runs only on CI.
  const chosen = await decide({ EVENT: "pull_request" }, { changedPaths: localPaths, treePaths, workspaceCrates, pictureInputs: async () => new Set() });
  console.log(`${chosen.full ? "Full suite" : "Selected"}: ${chosen.reason}. ${SELECTABLE.map((job) => `${job}=${chosen[job]}`).join(" ")}`);
} else if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const chosen = await decide(process.env);
  const line = `${chosen.full ? "Full suite" : "Selected"}: ${chosen.reason}. ${SELECTABLE.map((job) => `${job}=${chosen[job]}`).join(" ")}`;
  console.log(line);
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${line}\n`);
  appendFileSync(process.env.GITHUB_OUTPUT, SELECTABLE.map((job) => `${job}=${chosen[job]}\n`).join(""));
}
