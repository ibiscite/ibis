#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { appendFileSync } from "node:fs";

// Shared runners only report the 100k open/refresh timings; the test enforces
// them where IBIS_ENFORCE_BUDGETS is set. Command, correctness, ranking and
// refresh/copy failures stay fatal everywhere.
const enforced = Boolean(process.env.IBIS_ENFORCE_BUDGETS);
const result = spawnSync("cargo", ["test", "-p", "ibis-engine", "--locked", "--release", "--test", "refresh_budget", "--", "open_refresh_budget", "--ignored", "--nocapture", "--test-threads=1"],
  { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
process.stdout.write(result.stdout ?? "");
process.stderr.write(result.stderr ?? "");
const numbers = (result.stdout ?? "").split("\n").filter((line) => line.startsWith("100k "));
if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY,
  `### 100k open and refresh\n${enforced ? "Enforced: each must stay below 3 seconds." : "Report only: IBIS_ENFORCE_BUDGETS is not set on this runner."}\n\n\`\`\`text\n${numbers.join("\n") || "No measurements produced."}\n\`\`\`\n`);
if (result.error || result.status !== 0 || !numbers.some((line) => line.startsWith("100k budget:"))) process.exitCode = 1;
