// Lists the work in flight from each open pull request's Handoff, and this checkout's worktrees.
// --write replaces the generated section of the tracking issue (label `tracking`).
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const REPO = process.env.GITHUB_REPOSITORY || "ibiscite/ibis";
// Only the maintainer's own branches and the release app count (AGENTS.md › Trust and authority).
const TRUSTED = new Set(["diyanko", "ibis-release[bot]"]);
export const START = "<!-- in-flight:start -->";
export const END = "<!-- in-flight:end -->";
const LIMIT = 500;

const gh = (args, input) => execFileSync("gh", ["api", ...args], { encoding: "utf8", input, maxBuffer: 1 << 26 });

// The text after a "Handoff" line (plain, bold or a heading) up to the end of its paragraph.
export function handoff(body = "") {
  const lines = body.replace(/\r/g, "").replace(/<!--[\s\S]*?-->/g, "").split("\n");
  const at = lines.findIndex((l) => /^\s*(#+\s*)?(\*\*)?handoff\b/i.test(l));
  if (at < 0) return "";
  const out = [];
  const first = lines[at].replace(/^\s*(#+\s*)?(\*\*)?handoff(\*\*)?\s*(\*\*)?:?(\*\*)?\s*/i, "");
  if (first.trim()) out.push(first.trim());
  for (const line of lines.slice(at + 1)) {
    const l = line.trim();
    if (!l) { if (out.length) break; continue; }
    if (/^(#|---|🤖|(refs|part of|closes|fixes|supersedes|tracking)\b)/i.test(l)) break;
    out.push(l);
  }
  const text = out.join(" ").replace(/\*\*/g, "").replace(/\s+/g, " ").trim();
  return text.length > LIMIT ? `${text.slice(0, text.lastIndexOf(" ", LIMIT))} …` : text;
}

export function section(pulls) {
  const rows = pulls
    .filter((p) => TRUSTED.has(p.user.login) && p.head.repo?.full_name === REPO)
    .sort((a, b) => b.number - a.number)
    .map((p) => {
      const state = p.draft ? "draft" : "ready";
      const note = handoff(p.body ?? "") || (p.user.login === "ibis-release[bot]" ? "Release pull request." : "No handoff yet.");
      return `- #${p.number} ${p.title} (${state}, \`${p.head.ref}\`, updated ${p.updated_at.slice(0, 10)}). ${note[0].toUpperCase()}${note.slice(1)}`;
    });
  return [
    START,
    "Generated from each open pull request's **Handoff** by `scripts/inflight.mjs`; change the pull request, not this list.",
    "",
    ...(rows.length ? rows : ["Nothing is in flight."]),
    END,
  ].join("\n");
}

export function replace(body, generated) {
  const from = body.indexOf(START);
  const to = body.indexOf(END);
  if (from < 0 || to < from) throw new Error(`the tracking issue needs ${START} and ${END} around its In flight list`);
  return body.slice(0, from) + generated + body.slice(to + END.length);
}

function local(open) {
  const git = (...args) => execFileSync("git", args, { encoding: "utf8" }).trim();
  const lines = [];
  for (const block of git("worktree", "list", "--porcelain").split("\n\n")) {
    const path = block.match(/^worktree (.*)$/m)?.[1];
    const branch = block.match(/^branch refs\/heads\/(.*)$/m)?.[1] ?? "(detached)";
    if (!path) continue;
    const dirty = git("-C", path, "status", "--porcelain").length > 0;
    const pr = open.get(branch);
    lines.push(`  ${path}  ${branch}${pr ? `  #${pr}` : ""}${dirty ? "  (uncommitted changes)" : ""}`);
  }
  const gone = git("for-each-ref", "--format=%(refname:short) %(upstream:track)", "refs/heads")
    .split("\n").filter((l) => l.endsWith("[gone]")).map((l) => `  ${l.split(" ")[0]}`);
  return ["", "This checkout's worktrees:", ...lines, ...(gone.length ? ["Local branches whose pull request merged (remove them):", ...gone] : [])].join("\n");
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const pulls = JSON.parse(gh([`repos/${REPO}/pulls?state=open&per_page=100`]));
  const generated = section(pulls);
  if (process.argv.includes("--write")) {
    const [issue] = JSON.parse(gh([`repos/${REPO}/issues?labels=tracking&state=open&per_page=1`]));
    if (!issue) throw new Error("no open issue has the tracking label");
    const body = replace(issue.body ?? "", generated);
    if (body !== issue.body) gh(["-X", "PATCH", `repos/${REPO}/issues/${issue.number}`, "--input", "-"], JSON.stringify({ body }));
    console.log(`#${issue.number} ${body === issue.body ? "already up to date" : "updated"}.`);
  } else {
    console.log(generated.split("\n").slice(3, -1).join("\n"));
    console.log(local(new Map(pulls.map((p) => [p.head.ref, p.number]))));
  }
}
