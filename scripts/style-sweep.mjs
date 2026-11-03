#!/usr/bin/env node
// Compares two all-styles renders from `cargo run --release -p ibis-csl --example
// style_sweep`, so a pin or engine update shows what people would see change.
//
//   node scripts/style-sweep.mjs compare OLD.jsonl NEW.jsonl REPORT.md
//
// Exits 1 when a style fails that is not listed as a known exception in
// crates/ibis-csl/tests/sweep-exceptions.json; REPORT.md is written either way.
import { readFileSync, writeFileSync } from "node:fs";
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
    fixed: exceptions.filter((style) => next.has(style) && !failures.includes(style)).sort(),
  };
}

const list = (styles) => {
  const shown = styles.slice(0, MAX_LISTED).map((style) => `\`${style}\``).join(", ");
  return styles.length > MAX_LISTED ? `${shown} and ${styles.length - MAX_LISTED} more` : shown;
};

export function report({ removed, added, changed, newFailures, fixed }, previous, next) {
  const lines = [`Output changed in ${changed.length} ${changed.length === 1 ? "style" : "styles"}.`];
  if (newFailures.length) lines.push("", `**New failures:** ${list(newFailures)}`);
  if (changed.length) lines.push("", `**Changed:** ${list(changed)}`);
  if (added.length) lines.push("", `**Added:** ${list(added)}`);
  if (removed.length) lines.push("", `**Removed:** ${list(removed)}`);
  if (fixed.length) lines.push("", `**No longer failing, so remove from the exceptions:** ${list(fixed)}`);
  for (const style of changed.slice(0, 20)) {
    lines.push("", `<details><summary>${style}</summary>`, "", "```diff");
    const before = previous.get(style);
    const after = next.get(style);
    const shown = (output) => parts(output).map((part) => (typeof part === "string" ? part : JSON.stringify(part)));
    const [old, updated] = [shown(before), shown(after)];
    for (const [index, line] of updated.entries()) {
      if (old[index] !== line) lines.push(`- ${old[index]}`, `+ ${line}`);
    }
    lines.push("```", "", "</details>");
  }
  return `${lines.join("\n")}\n`;
}

function main() {
  const [command, oldPath, newPath, reportPath] = process.argv.slice(2);
  if (command !== "compare" || !reportPath) {
    console.error("Usage: node scripts/style-sweep.mjs compare OLD.jsonl NEW.jsonl REPORT.md");
    process.exit(2);
  }
  const root = join(dirname(fileURLToPath(import.meta.url)), "..");
  const exceptions = JSON.parse(readFileSync(join(root, "crates/ibis-csl/tests/sweep-exceptions.json"), "utf8"));
  const previous = readSweep(readFileSync(oldPath, "utf8"));
  const next = readSweep(readFileSync(newPath, "utf8"));
  const result = compareSweeps(previous, next, exceptions);
  writeFileSync(reportPath, report(result, previous, next));
  console.log(`${next.size} styles, ${result.changed.length} changed, ${result.newFailures.length} new failures`);
  process.exit(result.newFailures.length ? 1 : 0);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main();
