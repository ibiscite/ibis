// Check only the pull request title. Reads --title/--branch or the Actions event.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const TYPES = "feat|fix|perf|docs|test|ci|build|refactor|chore|revert";
const TITLE = new RegExp(`^(${TYPES})(\\([a-z0-9-]+\\))?!?: (\\S.*)$`);
const BOT_BRANCH = /^(release-please--|dependabot\/)/;
// GitHub's Revert button: `Revert "fix: …"` from `revert-<PR>-<branch>`; reverting a red main must never wait on lint.
const REVERT = { title: /^Revert "/, branch: /^revert-\d+-/ };

// The title becomes the squash commit and release notes.
export function lint({ title = "", branch = "" }) {
  const errors = [];
  const warnings = [];
  if (BOT_BRANCH.test(branch)) return { errors, warnings };
  if (REVERT.branch.test(branch) && REVERT.title.test(title.trim())) return { errors, warnings };

  const t = title.trim().match(TITLE);
  if (!t) errors.push(`title must be a Conventional Commit such as "fix: keep notes as literal text" (types: ${TYPES.replaceAll("|", ", ")})`);
  else {
    if (title.trim().length > 72) errors.push(`title is ${title.trim().length} characters; at most 72`);
    if (/\.$/.test(t[3])) errors.push("title must not end with a full stop");
    if (/^\p{Lu}\p{Ll}/u.test(t[3])) warnings.push("title should start in lower case after the colon");
  }

  return { errors, warnings };
}

function fromEvent(path) {
  const pr = JSON.parse(readFileSync(path, "utf8")).pull_request;
  if (!pr) throw new Error("GITHUB_EVENT_PATH holds no pull_request event");
  return { title: pr.title, branch: pr.head.ref };
}

function fromArgs(argv) {
  const get = (name) => {
    const at = argv.indexOf(name);
    return at >= 0 ? argv[at + 1] : undefined;
  };
  return { title: get("--title") ?? "", branch: get("--branch") ?? "" };
}

function main() {
  const argv = process.argv.slice(2);
  const input = argv.length === 0 && process.env.GITHUB_EVENT_PATH ? fromEvent(process.env.GITHUB_EVENT_PATH) : fromArgs(argv);
  const { errors, warnings } = lint(input);
  const ci = Boolean(process.env.GITHUB_ACTIONS);
  for (const w of warnings) console.log(ci ? `::warning title=Pull request conventions::${w}` : `warning: ${w}`);
  for (const e of errors) console.log(ci ? `::error title=Pull request conventions::${e}` : `error: ${e}`);
  console.log(errors.length ? "See CONTRIBUTING.md#commits-and-pull-requests." : "Pull request follows the conventions.");
  if (errors.length) process.exitCode = 1;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main();
