// Nightly publication needs successful push CI on main for its own commit; a
// later commit on main does not hold it back. Missing, pending or unreadable
// evidence never authorizes replacing downloads.
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

export function requireMainCi(repository, sha, read = (path) => {
  const result = spawnSync("gh", ["api", `repos/${repository}/${path}`], { encoding: "utf8", timeout: 60_000, maxBuffer: 1024 * 1024 });
  if (result.status !== 0) throw new Error("CI lookup unavailable (infrastructure)");
  return JSON.parse(result.stdout);
}) {
  if (!/^[\w.-]+\/[\w.-]+$/.test(repository ?? "") || !/^[a-f0-9]{40}$/.test(sha ?? "")) throw new Error("Invalid repository or candidate SHA");
  const runs = read(`actions/workflows/ci.yml/runs?event=push&branch=main&head_sha=${sha}&per_page=1`)?.workflow_runs;
  const latest = runs?.[0];
  if (!Array.isArray(runs) || runs.length !== 1 || latest.event !== "push" || latest.head_branch !== "main" || latest.head_sha !== sha
    || latest.status !== "completed" || latest.conclusion !== "success") throw new Error("Main push CI has not passed for this commit; the preceding nightly stays");
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try { requireMainCi(process.env.REPO, process.env.SHA); console.log("Main push CI: passed"); }
  catch (error) { console.error(`::error::${error.message}`); process.exitCode = 1; }
}
