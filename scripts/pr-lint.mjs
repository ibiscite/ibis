// Pull request conventions from "Pull requests" in docs/development.md (#707).
//
//   node scripts/pr-lint.mjs --title "fix: …" --body-file pr.md --branch fix/12-name
//   node scripts/pr-lint.mjs            in Actions: reads the pull_request event
//                                       from GITHUB_EVENT_PATH (no API, no token)
//
// Errors exit 1; warnings are printed and do not fail. Bot pull requests
// (Release Please, Dependabot) and GitHub's Revert button skip conventions,
// but their commit identities are still checked.

import { testDiff, testTamper } from "./test-tamper.mjs";
import { checkAgentIdentities } from "./check-agent-identity.mjs";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const TYPES = "feat|fix|perf|docs|test|ci|build|refactor|chore|revert";
const TITLE = new RegExp(`^(${TYPES})(\\([a-z0-9-]+\\))?!?: (\\S.*)$`);
const BRANCH = new RegExp(`^(${TYPES})/(\\d+)-[a-z0-9][a-z0-9-]*$`);
const BOT_BRANCH = /^(release-please--|dependabot\/)/;
// GitHub's Revert button: `Revert "fix: …"` from `revert-<PR>-<branch>`; reverting a red main must never wait on lint.
const REVERT = { title: /^Revert "/, branch: /^revert-\d+-/ };
const LINK = /^[ \t]*(?:closes|fixes|resolves|part of) #(\d+)\b/gim;

// Until 1.0 only the title (it becomes the release notes), commit identities and
// weakened tests fail the check (ADR 0027); everything else is a warning.
export function lint({ title = "", body = "", branch = "", fork = false, files = [] }) {
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

  if (!fork && !BRANCH.test(branch)) warnings.push(`branch \`${branch}\` could be named <type>/<issue>-<short-name>, for example fix/12-duplicate-keys`);

  const links = [...body.replace(/<!--[\s\S]*?(-->|$)/g, " ").matchAll(LINK)];
  if (links.length === 0) warnings.push('body has no "Closes #N" or "Part of #N" line');

  if (/requested by|project thread/i.test(body) || /^#{1,6}.*\b(pause|paused|resume)\b/im.test(body)) {
    warnings.push("body contains session or pause notes; keep them out of pull requests");
  }
  errors.push(...testTamper(files, body));
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
  try { errors.push(...checkAgentIdentities().errors); }
  catch (error) { errors.push(error.message); }
  if (!BOT_BRANCH.test(input.branch) && !(REVERT.branch.test(input.branch) && REVERT.title.test(input.title.trim()))) {
    try {
      const event = argv.length === 0 && process.env.GITHUB_EVENT_PATH ? JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, "utf8")) : null;
      errors.push(...testTamper(testDiff(event), input.body));
    } catch (error) { errors.push(error.message); }
  }
  const ci = Boolean(process.env.GITHUB_ACTIONS);
  for (const w of warnings) console.log(ci ? `::warning title=Pull request conventions::${w}` : `warning: ${w}`);
  for (const e of errors) console.log(ci ? `::error title=Pull request conventions::${e}` : `error: ${e}`);
  console.log(errors.length ? "See docs/development.md#pull-requests." : "Pull request follows the conventions.");
  if (errors.length) process.exitCode = 1;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main();
