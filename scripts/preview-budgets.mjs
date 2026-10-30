#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { appendFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

export function budgetTest(ref) {
  if (/^refs\/heads\/[^\s]+$/.test(ref)) return "open_refresh_report";
  const version = /^refs\/tags\/v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?$/.exec(ref);
  if (!version) throw new Error("Invalid build ref");
  return BigInt(version[1]) > 0n || BigInt(version[2]) >= 3n ? "open_refresh_budget" : "open_refresh_report";
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const name = budgetTest(process.argv[2] ?? "");
  const result = spawnSync("cargo", ["test", "-p", "ibis-engine", "--locked", "--release", "--test", "refresh_budget", "--", name, "--ignored", "--nocapture", "--test-threads=1"],
    { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
  process.stdout.write(result.stdout ?? "");
  process.stderr.write(result.stderr ?? "");
  const numbers = (result.stdout ?? "").split("\n").filter((line) => line.startsWith("100k "));
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY,
    `### 100k open and refresh\n${name === "open_refresh_budget" ? "Enforced: each must stay below 3 seconds." : "Report only: release limits begin at 0.3."}\n\n\`\`\`text\n${numbers.join("\n") || "No measurements produced."}\n\`\`\`\n`);
  // Report mode removes only the two timing assertions. Command, correctness,
  // ranking, search and refresh/copy failures remain fatal on every channel.
  if (result.error || result.status !== 0 || !numbers.some((line) => line.startsWith("100k budget:"))) process.exitCode = 1;
}
