// Publication needs successful push CI for the candidate and current main.
// Missing, pending or unreadable evidence never authorizes replacing downloads.
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

export function requireMainCi(repository, sha, read = (path) => {
  const result = spawnSync("gh", ["api", `repos/${repository}/${path}`], { encoding: "utf8", timeout: 60_000, maxBuffer: 1024 * 1024 });
  if (result.status !== 0) throw new Error("CI lookup unavailable (infrastructure)");
  return JSON.parse(result.stdout);
}) {
  if (!/^[\w.-]+\/[\w.-]+$/.test(repository ?? "") || !/^[a-f0-9]{40}$/.test(sha ?? "")) throw new Error("Invalid repository or candidate SHA");
  const main = read("git/ref/heads/main")?.object;
  if (main?.type !== "commit" || !/^[a-f0-9]{40}$/.test(main.sha ?? "")) throw new Error("Main revision unavailable");
  for (const head of new Set([sha, main.sha])) {
    const runs = read(`actions/workflows/ci.yml/runs?event=push&branch=main&head_sha=${head}&per_page=1`)?.workflow_runs;
    const latest = runs?.[0];
    if (!Array.isArray(runs) || runs.length !== 1 || latest.event !== "push" || latest.head_branch !== "main" || latest.head_sha !== head
      || latest.status !== "completed" || latest.conclusion !== "success") throw new Error("Main push CI has not passed; keep the preceding nightly and rerun after CI is green");
  }
  if (read("git/ref/heads/main")?.object?.sha !== main.sha) throw new Error("Main changed during CI lookup; retry publication");
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try { requireMainCi(process.env.REPO, process.env.SHA); console.log("Main push CI: passed"); }
  catch (error) { console.error(`::error::${error.message}`); process.exitCode = 1; }
}
