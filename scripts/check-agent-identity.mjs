// Public maintainer metadata only; rejected values never reach diagnostics.
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const SHA = /^[a-f0-9]{40}$/;
const maintainer = (name) => ["diyanko bhowmik", "diyanko", "@diyanko"].includes(name.normalize("NFKC").trim().replace(/\s+/g, " ").toLowerCase());
const publicEmail = (email) => /^(?:diyanko@ibiscite\.org|noreply@github\.com|[^\s<>@]+@users\.noreply\.github\.com)$/i.test(email);
// Co-authorship can disclose any AI product; never depend on a brand allowlist.
const assisted = /^(?:Assisted-by|Co-authored-by)[ \t]*:/im;
const fail = (message) => { throw new Error(`Commit identity check: ${message}`); };

function git(args) {
  if (["GIT_DIR", "GIT_WORK_TREE", "GIT_COMMON_DIR", "GIT_INDEX_FILE", "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_SHALLOW_FILE", "GIT_GRAFT_FILE"].some((key) => process.env[key])) fail("Git source overrides are unsupported.");
  const result = spawnSync("git", ["--no-pager", "--no-replace-objects", "-c", "core.fsmonitor=false", "-c", "log.showSignature=false", ...args], {
    encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], timeout: 10_000, killSignal: "SIGKILL", maxBuffer: 4 * 1024 * 1024,
    env: { ...process.env, GIT_NO_LAZY_FETCH: "1" },
  });
  if (result.error || result.status !== 0) fail("intended commits are unavailable; fetch the full base/head history and retry.");
  return result.stdout.trimEnd();
}

function eventInput() {
  if (!process.env.GITHUB_EVENT_PATH) {
    if (process.env.GITHUB_ACTIONS) fail("the Actions event is missing.");
    return null;
  }
  try {
    const event = JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, "utf8"));
    if (!event || typeof event !== "object" || Array.isArray(event)) throw new Error();
    return event;
  }
  catch { fail("the Actions event is unreadable or invalid."); }
}

export function checkAgentIdentities(event = eventInput()) {
  if (process.env.GITHUB_EVENT_NAME === "pull_request" && !event?.pull_request || process.env.GITHUB_EVENT_NAME === "merge_group" && !event?.merge_group) fail("the intended event range is missing.");
  if (git(["rev-parse", "--is-shallow-repository"]) !== "false") fail("full history is required; shallow inputs cannot establish the intended range.");
  const head = git(["rev-parse", "--verify", "HEAD^{commit}"]);
  const verify = (sha) => {
    if (!SHA.test(sha ?? "")) fail("a full base/head commit ID is required.");
    if (git(["rev-parse", "--verify", `${sha}^{commit}`]) !== sha) fail("the intended commit does not match.");
    return sha;
  };
  if (process.env.GITHUB_SHA && verify(process.env.GITHUB_SHA) !== head) fail("the checkout does not match the event head.");
  let base, tip = head, expected, candidate = null;
  if (event?.pull_request) {
    const pr = event.pull_request;
    verify(pr.base?.sha); tip = verify(pr.head?.sha);
    expected = pr.commits;
    if (!Number.isSafeInteger(expected) || expected < 1) fail("the PR commit count is missing or invalid.");
    const parents = git(["show", "-s", "--format=%P", head]).split(" ");
    if (parents.length !== 2 || parents[1] !== tip) fail("the checkout must be the matching PR merge candidate.");
    base = verify(parents[0]); // Current merge base can be newer than the event base.
    git(["merge-base", "--is-ancestor", pr.base.sha, base]);
  } else if (event?.merge_group) {
    base = verify(event.merge_group.base_sha); tip = verify(event.merge_group.head_sha);
    if (tip !== head) fail("the checkout must match the merge-group candidate.");
    candidate = head; // Automated candidate metadata matters even without an AI trailer.
  } else if (!event) {
    base = git(["merge-base", "origin/main", head]);
  }
  let commits = [head];
  if (base) {
    git(["merge-base", "--is-ancestor", base, event?.pull_request ? head : tip]);
    commits = git(["rev-list", "--reverse", `${base}..${tip}`]).split("\n").filter(Boolean);
    if (event && commits.length === 0 || expected !== undefined && commits.length !== expected) fail("the intended commit range is empty or incomplete.");
    if (!commits.length) commits = [head];
  }
  const errors = [];
  for (const [index, commit] of commits.entries()) {
    const parts = git(["show", "-s", "--no-show-signature", "--no-use-mailmap", "--format=%an%x00%ae%x00%cn%x00%ce%x00%B", verify(commit)]).split("\0");
    if (parts.length !== 5) fail("commit metadata is malformed.");
    const [author, authorEmail, committer, committerEmail, body] = parts;
    const message = body.replace(/\r?\n[ \t]+/g, " "); // Git trailers can fold onto continuation lines.
    if (commit !== candidate && !assisted.test(message) && !/^(?:Codex|Claude|ChatGPT|GitHub Copilot)\b/i.test(author)) continue;
    const identities = [["author", author, authorEmail], ["committer", committer, committerEmail]];
    for (const match of message.matchAll(/^(Signed-off-by|Co-authored-by)[ \t]*:[ \t]*([^\r\n]*)/gim)) {
      const identity = match[2].match(/^(.*?)\s*<([^<>]*)>\s*$/);
      identities.push([match[1].toLowerCase(), identity?.[1]?.trim() ?? match[2].split("<")[0].trim(), identity?.[2] ?? ""]);
    }
    for (const [role, name, email] of identities) if (maintainer(name) && !publicEmail(email)) {
      errors.push(`Commit identity ${index + 1}: maintainer ${role} must use the public project or GitHub noreply address.`);
    }
  }
  return { errors, count: commits.length };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    if (process.argv.length !== 2) fail("this guard takes no arguments; it uses the checkout and Actions event.");
    const { errors, count } = checkAgentIdentities();
    if (errors.length) { console.error(errors.join("\n")); process.exitCode = 1; }
    else console.log(`Commit identities checked (${count} commits).`);
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
