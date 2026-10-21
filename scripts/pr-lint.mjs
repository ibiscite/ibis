// Pull request conventions from "Pull requests" in docs/development.md (#707).
//
//   node scripts/pr-lint.mjs --title "fix: …" --body-file pr.md --branch fix/12-name
//   node scripts/pr-lint.mjs            in Actions: reads the pull_request event
//                                       from GITHUB_EVENT_PATH (no API, no token)
//
// Errors exit 1; warnings are printed and do not fail. Bot pull requests
// (Release Please, Dependabot) and GitHub's Revert button are skipped.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const TYPES = "feat|fix|perf|docs|test|ci|build|refactor|chore|revert";
const TITLE = new RegExp(`^(${TYPES})(\\([a-z0-9-]+\\))?!?: (\\S.*)$`);
const BRANCH = new RegExp(`^(${TYPES})/(\\d+)-[a-z0-9][a-z0-9-]*$`);
const BOT_BRANCH = /^(release-please--|dependabot\/)/;
// GitHub's Revert button: `Revert "fix: …"` from `revert-<PR>-<branch>`; reverting a red main must never wait on lint.
const REVERT = { title: /^Revert "/, branch: /^revert-\d+-/ };
const LINK = /^[ \t]*(?:closes|fixes|resolves|part of) #(\d+)\b/gim;
const SECTIONS = ["What and why", "How it works", "Verification"];
const MAX_WORDS = 250;
const HARD_MAX_WORDS = 300;

export function countWords(body) {
  return body
    .replace(/<!--[\s\S]*?(-->|$)/g, " ")
    .replace(/!\[[^\]]*\]\([^)]*\)/g, " ")
    .replace(/<img\b[^>]*>/gi, " ")
    .split("\n")
    .filter((line) => !/^\s*(#{1,6}\s|[-*] \[[ xX]\])/.test(line))
    .join(" ")
    .split(/\s+/)
    .filter((word) => /[\p{L}\p{N}]/u.test(word)).length;
}

export function lint({ title = "", body = "", branch = "", fork = false }) {
  const errors = [];
  const warnings = [];
  if (BOT_BRANCH.test(branch)) return { errors, warnings };
  if (REVERT.branch.test(branch) && REVERT.title.test(title.trim())) return { errors, warnings };
  body = body ?? "";

  const t = title.trim().match(TITLE);
  if (!t) errors.push(`title must be a Conventional Commit such as "fix: keep notes as literal text" (types: ${TYPES.replaceAll("|", ", ")})`);
  else {
    if (title.trim().length > 72) errors.push(`title is ${title.trim().length} characters; at most 72`);
    if (/\.$/.test(t[3])) errors.push("title must not end with a full stop");
    if (/^\p{Lu}\p{Ll}/u.test(t[3])) warnings.push("title should start in lower case after the colon");
  }

  const b = branch.match(BRANCH);
  if (!fork) {
    if (!b) errors.push(`branch \`${branch}\` must be <type>/<issue>-<short-name>, for example fix/12-duplicate-keys`);
    else if (t && b[1] !== t[1]) errors.push(`branch type \`${b[1]}\` differs from title type \`${t[1]}\``);
  }

  const links = [...body.replace(/<!--[\s\S]*?(-->|$)/g, " ").matchAll(LINK)].map((m) => m[1]);
  if (links.length !== 1) errors.push(`body needs exactly one "Closes #N" or "Part of #N" line; found ${links.length}`);
  else if (b && !fork && links[0] !== b[2]) warnings.push(`body links #${links[0]} but the branch names #${b[2]}`);

  const words = countWords(body);
  if (words > HARD_MAX_WORDS) errors.push(`body has ${words} words; at most ${MAX_WORDS} (move evidence to an "Evidence for <short SHA>" comment)`);
  else if (words > MAX_WORDS) warnings.push(`body has ${words} words; aim for at most ${MAX_WORDS}`);

  if (/\/actions\/runs\/\d+/.test(body)) errors.push('body links a workflow run; put run links in an "Evidence for <short SHA>" comment');
  const shas = new Set(body.match(/\b[0-9a-f]{40}\b/g) ?? []);
  if (shas.size > 1) errors.push(`body has ${shas.size} full commit hashes; put evidence in a comment`);

  for (const section of SECTIONS) {
    if (!new RegExp(`^#{2,3}\\s+${section}\\s*$`, "mi").test(body)) errors.push(`body is missing the "## ${section}" section from the template`);
  }
  if (/requested by|project thread/i.test(body) || /^#{1,6}.*\b(pause|paused|resume)\b/im.test(body)) {
    warnings.push("body contains session or pause notes; keep them out of pull requests");
  }
  return { errors, warnings };
}

function fromEvent(path) {
  const pr = JSON.parse(readFileSync(path, "utf8")).pull_request;
  if (!pr) throw new Error("GITHUB_EVENT_PATH holds no pull_request event");
  return { title: pr.title, body: pr.body ?? "", branch: pr.head.ref, fork: pr.head.repo?.full_name !== pr.base.repo.full_name };
}

function fromArgs(argv) {
  const get = (name) => {
    const at = argv.indexOf(name);
    return at >= 0 ? argv[at + 1] : undefined;
  };
  const bodyFile = get("--body-file");
  return { title: get("--title") ?? "", body: bodyFile ? readFileSync(bodyFile, "utf8") : "", branch: get("--branch") ?? "" };
}

function main() {
  const argv = process.argv.slice(2);
  const input = argv.length === 0 && process.env.GITHUB_EVENT_PATH ? fromEvent(process.env.GITHUB_EVENT_PATH) : fromArgs(argv);
  const { errors, warnings } = lint(input);
  const ci = Boolean(process.env.GITHUB_ACTIONS);
  for (const w of warnings) console.log(ci ? `::warning title=Pull request conventions::${w}` : `warning: ${w}`);
  for (const e of errors) console.log(ci ? `::error title=Pull request conventions::${e}` : `error: ${e}`);
  console.log(errors.length ? "See docs/development.md#pull-requests." : "Pull request follows the conventions.");
  if (errors.length) process.exitCode = 1;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main();
