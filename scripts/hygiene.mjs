// Weekly repository hygiene (#443), run by .github/workflows/hygiene.yml.
//
//   node scripts/hygiene.mjs           report only: prints what it would do
//   node scripts/hygiene.mjs --apply   deletes finished branches and updates
//                                      the "Repository hygiene report" issue
//
// Needs the GitHub CLI signed in (GH_TOKEN in CI) and GITHUB_REPOSITORY or
// --repo owner/name. The label standard below mirrors "Issues, labels and
// milestones" in docs/development.md; change both together.

import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const LABEL_FAMILIES = /^(type|area|platform|source|status|needs):/;
const LABEL_EXTRAS = new Set([
  "tracking", "good first issue", "help wanted", "upstream",
  "autorelease: pending", "autorelease: tagged", "dependencies",
]);
const KEEP_BRANCH = /^(main|nightly|stable|evidence|release-please--.*|dependabot\/.*)$/;
const TITLE_PREFIX = /^\s*(\[|(feat|fix|perf|docs|test|ci|build|refactor|chore|revert)(\([^)]*\))?!?:)/;
const DAY = 86_400_000;
const STALE_DAYS = 14;
const REPORT_TITLE = "Repository hygiene report";

export function findings({ now, branches, pulls, issues, labels }) {
  const out = { deleteBranches: [], problems: [] };
  const openBases = new Set(pulls.filter((p) => p.state === "open").map((p) => p.base));
  for (const { name, sha } of branches) {
    if (KEEP_BRANCH.test(name) || openBases.has(name)) continue;
    const own = pulls.filter((p) => p.head === name);
    if (own.length === 0 || own.some((p) => p.state === "open")) continue;
    const last = own.reduce((a, b) => (Date.parse(b.closedAt) > Date.parse(a.closedAt) ? b : a));
    // Commits pushed after the pull request closed are live work: keep them.
    if (last.headSha === sha && now - Date.parse(last.closedAt) > DAY) out.deleteBranches.push(name);
  }
  for (const p of pulls) {
    if (p.state === "open" && !KEEP_BRANCH.test(p.head) && now - Date.parse(p.updatedAt) > STALE_DAYS * DAY) {
      out.problems.push(`#${p.number}: pull request with no activity for ${STALE_DAYS} days`);
    }
    if (p.state === "open" && !KEEP_BRANCH.test(p.head) && !p.labels.some((l) => l.startsWith("type:"))) {
      out.problems.push(`#${p.number}: pull request with no type: label`);
    }
  }
  for (const i of issues) {
    if (i.title === REPORT_TITLE) continue;
    if (!i.labels.some((l) => l.startsWith("type:"))) out.problems.push(`#${i.number}: no type: label`);
    if (!i.milestone) out.problems.push(`#${i.number}: no milestone`);
    if (TITLE_PREFIX.test(i.title)) out.problems.push(`#${i.number}: title starts with a commit prefix`);
  }
  for (const name of labels) {
    if (!LABEL_FAMILIES.test(name) && !LABEL_EXTRAS.has(name)) out.problems.push(`label \`${name.replaceAll("`", "")}\` is outside the naming standard`);
  }
  return out;
}

function gh(args, input) {
  return execFileSync("gh", args, { encoding: "utf8", input, maxBuffer: 1 << 30, stdio: ["pipe", "pipe", "inherit"] });
}

function list(repo, path) {
  const all = [];
  for (let page = 1; ; page++) {
    const items = JSON.parse(gh(["api", `repos/${repo}/${path}&per_page=100&page=${page}`]));
    all.push(...items);
    if (items.length < 100) return all;
  }
}

function main() {
  const apply = process.argv.includes("--apply");
  const at = process.argv.indexOf("--repo");
  const repo = at > 0 ? process.argv[at + 1] : process.env.GITHUB_REPOSITORY;
  if (!repo) throw new Error("set GITHUB_REPOSITORY or pass --repo owner/name");

  const pulls = list(repo, "pulls?state=all")
    .map((p) => ({
      number: p.number,
      head: p.head.repo?.full_name === repo ? p.head.ref : null,
      headSha: p.head.sha,
      base: p.base.ref,
      state: p.state,
      closedAt: p.closed_at,
      updatedAt: p.updated_at,
      labels: p.labels.map((l) => l.name),
    }));
  const issues = list(repo, "issues?state=open")
    .filter((i) => !i.pull_request)
    .map((i) => ({ number: i.number, title: i.title, labels: i.labels.map((l) => l.name), milestone: i.milestone?.title }));
  const branches = list(repo, "branches?").map((b) => ({ name: b.name, sha: b.commit.sha }));
  const labels = list(repo, "labels?").map((l) => l.name);

  const result = findings({ now: Date.now(), branches, pulls, issues, labels });
  for (const branch of result.deleteBranches) {
    console.log(`${apply ? "Deleting" : "Would delete"} branch ${branch}`);
    if (!apply) continue;
    try {
      gh(["api", "-X", "DELETE", `repos/${repo}/git/refs/heads/${branch.split("/").map(encodeURIComponent).join("/")}`]);
    } catch {
      result.problems.push(`branch \`${branch}\` could not be deleted`);
      process.exitCode = 1;
    }
  }
  for (const problem of result.problems) console.log(problem);
  console.log(`Checked ${branches.length} branches, ${pulls.length} pull requests, ${issues.length} open issues and ${labels.length} labels: ${result.deleteBranches.length} finished branches, ${result.problems.length} problems.`);

  const report = issues.find((i) => i.title === REPORT_TITLE);
  const body = `Found by the weekly hygiene run. Fix these, then the next run closes this issue.\n\n${result.problems.map((p) => `- ${p}`).join("\n")}\n`;
  if (!apply) return;
  if (result.problems.length && report) gh(["api", "-X", "PATCH", `repos/${repo}/issues/${report.number}`, "-f", `body=${body}`]);
  else if (result.problems.length) gh(["api", `repos/${repo}/issues`, "-f", `title=${REPORT_TITLE}`, "-f", `body=${body}`, "-f", "labels[]=type:chore", "-f", "labels[]=needs:triage"]);
  else if (report) gh(["api", "-X", "PATCH", `repos/${repo}/issues/${report.number}`, "-f", "state=closed", "-f", "state_reason=completed"]);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main();
