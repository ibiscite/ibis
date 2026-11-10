#!/usr/bin/env node
// Renders the fixture items in every bundled CSL style (`cargo run --release -p
// ibis-csl --example style_sweep`) at two points and compares them, so a pin or
// engine update shows what people would see change (#19).
//
//   node scripts/style-sweep.mjs weekly OUT
//     Renders at the pins, moves the styles, locales and citeproc-js to
//     upstream's newest (csl-bundle.mjs --update, vendor-citeproc.mjs --newest)
//     and renders again. Writes OUT/report.md, and OUT/title.txt only when the
//     pins moved; the moved files stay in this checkout for the pull request.
//   node scripts/style-sweep.mjs release OLD_REF OUT
//     Renders OLD_REF's engine and bundle, then this checkout's, both with this
//     checkout's sweep. Writes OUT/report.md and OUT/notes.md, the release
//     notes' list of styles whose output changed.
//   node scripts/style-sweep.mjs compare OLD.jsonl NEW.jsonl REPORT.md
//
// Each exits 1 when a style fails that is not listed as a known exception in
// crates/ibis-csl/tests/sweep-exceptions.json, after writing its files.
import { spawnSync } from "node:child_process";
import { closeSync, copyFileSync, mkdirSync, mkdtempSync, openSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const MAX_LISTED = 200;

export function readSweep(text) {
  return new Map(text.split("\n").filter(Boolean).map((line) => {
    const { style, output } = JSON.parse(line);
    return [style, output];
  }));
}

/** Outputs a style could not produce, by its own fault rather than a refusal. */
const parts = (output) => [...output.citations, output.bibliography, ...(output.forms ?? []).flat()];

function failed(output) {
  return parts(output).some((part) => part?.failed !== undefined);
}

/** Text with a part missing: empty brackets or a separator where a name was. */
export const SUSPICIOUS = /\(\s*[,;:.]|[,;]\s*\)|^\s*[,;:.]|\(\s*\)/;

function suspicious(output) {
  return parts(output).some((part) => typeof part === "string" && part.split("\n").some((line) => SUSPICIOUS.test(line)));
}

export function compareSweeps(previous, next, exceptions) {
  const removed = [...previous.keys()].filter((style) => !next.has(style)).sort();
  const added = [...next.keys()].filter((style) => !previous.has(style)).sort();
  const changed = [...next.keys()].filter((style) => previous.has(style)
    && JSON.stringify(previous.get(style)) !== JSON.stringify(next.get(style))).sort();
  const failures = [...next].filter(([, output]) => failed(output)).map(([style]) => style).sort();
  const known = new Set(exceptions);
  return {
    removed,
    added,
    changed,
    failures,
    newFailures: failures.filter((style) => !known.has(style)),
    suspicious: [...next].filter(([, output]) => suspicious(output)).map(([style]) => style).sort(),
    fixed: exceptions.filter((style) => next.has(style) && !failures.includes(style)).sort(),
  };
}

const list = (styles) => {
  const shown = styles.slice(0, MAX_LISTED).map((style) => `\`${style}\``).join(", ");
  return styles.length > MAX_LISTED ? `${shown} and ${styles.length - MAX_LISTED} more` : shown;
};

/** A code fence no run of backticks in `text` can close. */
export function fence(text) {
  const longest = Math.max(0, ...[...text.matchAll(/`+/g)].map(([run]) => run.length));
  return "`".repeat(Math.max(3, longest + 1));
}

/** One style's changed parts as a diff, every line prefixed, inside a fence of its own. */
function details(style, before, after) {
  const shown = (output) => parts(output).map((part) => (typeof part === "string" ? part : JSON.stringify(part)));
  const [old, updated] = [shown(before), shown(after)];
  const prefixed = (mark, text) => String(text).split("\n").map((line) => `${mark} ${line}`);
  const diff = [];
  for (const [index, line] of updated.entries()) {
    if (old[index] !== line) diff.push(...prefixed("-", old[index]), ...prefixed("+", line));
  }
  const body = diff.join("\n");
  const marks = fence(body);
  return ["", `<details><summary>${style}</summary>`, "", `${marks}diff`, body, marks, "", "</details>"].join("\n");
}

/**
 * The comparison for a person. Each style's diff is whole or left out, so
 * `limit` (characters) never cuts one open.
 */
export function report({ removed, added, changed, newFailures, fixed, suspicious = [] }, previous, next, limit = Infinity) {
  const lines = [`Output changed in ${changed.length} ${changed.length === 1 ? "style" : "styles"}.`];
  if (newFailures.length) lines.push("", `**New failures:** ${list(newFailures)}`);
  if (suspicious.length) lines.push("", `**Text that looks incomplete, to check by hand:** ${list(suspicious)}`);
  if (changed.length) lines.push("", `**Changed:** ${list(changed)}`);
  if (added.length) lines.push("", `**Added:** ${list(added)}`);
  if (removed.length) lines.push("", `**Removed:** ${list(removed)}`);
  if (fixed.length) lines.push("", `**No longer failing, so remove from the exceptions:** ${list(fixed)}`);
  let text = lines.join("\n");
  let shown = 0;
  for (const style of changed.slice(0, 20)) {
    const part = details(style, previous.get(style), next.get(style));
    if (text.length + part.length > limit) break;
    text += part;
    shown += 1;
  }
  if (shown < Math.min(changed.length, 20)) text += `\n\nThe diffs of ${Math.min(changed.length, 20) - shown} more styles are in the full report.`;
  return `${text}\n`;
}

/** The weekly pull request's description: what to do first, then the bounded report. */
export function pullRequestBody(result, previous, next, run) {
  const lines = [
    "## Handoff",
    "",
    "Review the changed output against the upstream changes, report style or engine bugs upstream, add any style that now fails to `crates/ibis-csl/tests/sweep-exceptions.json` with its upstream report, then merge on green. A newer weekly run replaces this branch only while it holds nothing but the workflow's own commit; it turns auto-merge off before it does.",
    "",
  ];
  if (result.newFailures.length) lines.push("**This run found new failures; fix or list them before merging.**", "");
  if (run) lines.push(`Full report: ${run}`, "");
  lines.push("## Output diff", "");
  // A pull request body holds 65,536 characters.
  return `${lines.join("\n")}\n${report(result, previous, next, 50_000)}`;
}

/** The pull request title for moved pins, short enough for the release notes. */
export function pinTitle(before, after) {
  const moved = [];
  if (before.styles.tag !== after.styles.tag) moved.push(`CSL styles ${after.styles.tag}`);
  if (before.locales.tag !== after.locales.tag) moved.push(`locales ${after.locales.tag}`);
  if (before.zotero.commit !== after.zotero.commit) moved.push(`citeproc ${after.zotero.commit.slice(0, 7)}`);
  return moved.length ? `fix: update to ${moved.join(", ")}` : null;
}

/** The release notes' part on styles: which ones a person will see change. */
export function styleNotes({ changed, added, removed }, since) {
  const lines = ["**Citation styles**", ""];
  if (!changed.length && !added.length && !removed.length) {
    lines.push(`Every style writes what it wrote in ${since}.`);
    return `${lines.join("\n")}\n`;
  }
  if (changed.length) {
    lines.push(`- Output changed since ${since} in ${changed.length} ${changed.length === 1 ? "style" : "styles"}: ${list(changed)}`);
  }
  if (added.length) lines.push(`- New: ${list(added)}`);
  if (removed.length) lines.push(`- Removed or renamed (old ids still work when upstream renamed them): ${list(removed)}`);
  return `${lines.join("\n")}\n`;
}

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const SWEEP_FILES = ["crates/ibis-csl/examples/style_sweep.rs", "crates/ibis-csl/tests/sweep-items.json"];

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { stdio: ["ignore", "inherit", "inherit"], ...options });
  if (result.status !== 0) throw new Error(`${command} ${args.join(" ")} failed`);
}

function capture(command, args, cwd = root) {
  const result = spawnSync(command, args, { cwd, encoding: "utf8" });
  if (result.status !== 0) throw new Error(`${command} ${args.join(" ")} failed`);
  return result.stdout.trim();
}

/** One all-styles render of the checkout at `cwd`, built in its own target folder. */
function render(cwd, out, target) {
  const file = openSync(out, "w");
  try {
    run("cargo", ["run", "--quiet", "--locked", "--release", "-p", "ibis-csl", "--example", "style_sweep"], {
      cwd,
      stdio: ["ignore", file, "inherit"],
      env: { ...process.env, CARGO_TARGET_DIR: target },
    });
  } finally {
    closeSync(file);
  }
}

function exceptions() {
  return JSON.parse(readFileSync(join(root, "crates/ibis-csl/tests/sweep-exceptions.json"), "utf8"));
}

function compareFiles(oldPath, newPath, reportPath) {
  const previous = readSweep(readFileSync(oldPath, "utf8"));
  const next = readSweep(readFileSync(newPath, "utf8"));
  const result = compareSweeps(previous, next, exceptions());
  writeFileSync(reportPath, report(result, previous, next));
  console.log(`${next.size} styles, ${result.changed.length} changed, ${result.newFailures.length} new failures`);
  return { ...result, previous, next };
}

const target = () => process.env.CARGO_TARGET_DIR || join(root, "target");
const manifest = () => JSON.parse(readFileSync(join(root, "crates/ibis-csl/vendor/manifest.json"), "utf8")).sources;

function weekly(out) {
  mkdirSync(out, { recursive: true });
  render(root, join(out, "pinned.jsonl"), target());
  const before = manifest();
  run("node", ["scripts/csl-bundle.mjs", "--update"], { cwd: root });
  run("node", ["scripts/vendor-citeproc.mjs", "--newest"], { cwd: root });
  const title = pinTitle(before, manifest());
  if (!title) return compareFiles(join(out, "pinned.jsonl"), join(out, "pinned.jsonl"), join(out, "report.md"));
  render(root, join(out, "newest.jsonl"), target());
  const result = compareFiles(join(out, "pinned.jsonl"), join(out, "newest.jsonl"), join(out, "report.md"));
  writeFileSync(join(out, "body.md"), pullRequestBody(result, result.previous, result.next, process.env.STYLE_SWEEP_RUN));
  writeFileSync(join(out, "title.txt"), `${title}\n`);
  return result;
}

/**
 * Renders `ref` into `out` in a worktree of its own, with the sweep files from
 * this checkout (`sweep` null) or from another commit. The worktree has its own
 * target folder: one shared with this checkout would mix their builds.
 */
function renderAt(ref, sweep, out) {
  const tree = mkdtempSync(join(tmpdir(), "ibis-style-sweep-"));
  run("git", ["worktree", "add", "--quiet", "--detach", tree, ref], { cwd: root });
  try {
    if (sweep === null) for (const path of SWEEP_FILES) copyFileSync(join(root, path), join(tree, path));
    else run("git", ["checkout", "--quiet", sweep, "--", ...SWEEP_FILES], { cwd: tree });
    render(tree, out, join(tree, "target"));
  } finally {
    run("git", ["worktree", "remove", "--force", tree], { cwd: root });
    rmSync(tree, { recursive: true, force: true });
  }
}

function release(oldRef, out) {
  mkdirSync(out, { recursive: true });
  const since = capture("git", ["describe", "--tags", "--always", oldRef]);
  // The same sweep on both sides, so only the engine and bundle can differ:
  // this checkout's, else the old release's when this one does not build there.
  for (const sweep of [null, oldRef]) {
    try {
      renderAt(oldRef, sweep, join(out, "old.jsonl"));
      if (sweep === null) render(root, join(out, "new.jsonl"), target());
      else renderAt("HEAD", sweep, join(out, "new.jsonl"));
    } catch {
      console.error(`::warning::The ${sweep === null ? "current" : `${since}`} sweep does not build on both sides.`);
      continue;
    }
    const result = compareFiles(join(out, "old.jsonl"), join(out, "new.jsonl"), join(out, "report.md"));
    writeFileSync(join(out, "notes.md"), styleNotes(result, since));
    return result;
  }
  // Without a comparison there are no notes: the release waits for a sweep that builds.
  writeFileSync(join(out, "report.md"), `No sweep builds against both ${since} and this release.\n`);
  console.error(`::error::No sweep builds against both ${since} and this release.`);
  return { newFailures: ["(no comparison)"] };
}

function main() {
  const [command, ...args] = process.argv.slice(2);
  const usage = {
    weekly: args.length === 1,
    release: args.length === 2,
    compare: args.length === 3,
  };
  if (!usage[command]) {
    console.error("Usage: node scripts/style-sweep.mjs weekly OUT | release OLD_REF OUT | compare OLD.jsonl NEW.jsonl REPORT.md");
    process.exit(2);
  }
  const result = command === "weekly" ? weekly(...args) : command === "release" ? release(...args) : compareFiles(...args);
  process.exit(result.newFailures.length ? 1 : 0);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main();
