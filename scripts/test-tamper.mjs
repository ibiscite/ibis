// Heuristic guard: legitimate test rewrites need a visible explanation, not a ban.
import { spawnSync } from "node:child_process";

const testFile = (name) => /(?:^|\/)(?:__tests__|tests|test)[\/]|(?:^|\/)test_[^/]+$|(?:[._](?:test|spec)\.[^/]+$)|\.rs$/.test(name);
const expectedFile = (name) => /(?:^|\/)(?:fixtures|proptest-regressions|snapshots)\/|\.snap(?:\.new)?$/.test(name);

export function testTamper(files, body = "") {
  const visible = body.replace(/<!--[\s\S]*?(?:-->|$)|```[\s\S]*?(?:```|$)|~~~[\s\S]*?(?:~~~|$)/g, "");
  if ([...visible.matchAll(/^[ \t]{0,3}Test changes:[ \t]*(.*)$/gmi)].some((match) => /\p{L}/u.test(match[1]))) return [];
  const flagged = files.some(({ filename, patch }) => {
    const name = filename.replaceAll("\\", "/");
    if (expectedFile(name)) return true;
    if (!testFile(name)) return false;
    const lines = patch.split(/\r?\n/).filter((line) => !/^(?:---|\+\+\+)/.test(line));
    const removed = lines.filter((line) => line.startsWith("-")).map((line) => line.slice(1)).join("\n");
    const added = lines.filter((line) => line.startsWith("+")).map((line) => line.slice(1)).join("\n");
    return /\b(?:assert(?:_\w+)?!?\s*(?:[.(]|[ \t]+)|expect\s*[.(]|self\.assert\w*\s*\()/.test(removed)
      || /^\s*\.\s*(?:not\s*\.\s*)?(?:to[A-Z]\w*|rejects|resolves)\b/m.test(removed)
      || /#\s*\[\s*ignore\b|\.\s*(?:skip|only)\b|\btodo\s*!\s*\(/.test(added);
  });
  return flagged ? ["Test changes need review: add a visible line 'Test changes: <explain why assertions, disabled tests or expected data changed>' to the PR body."] : [];
}

function git(args) {
  const result = spawnSync("git", ["--literal-pathspecs", "-c", "core.fsmonitor=false", ...args], {
    encoding: "utf8", timeout: 30000, killSignal: "SIGKILL", maxBuffer: 32 * 1024 * 1024,
  });
  if (result.status !== 0 || result.error) throw new Error("Test diff unavailable. Run git fetch origin main, then retry; in CI check the checkout and origin access.");
  return result.stdout;
}

export function testDiff(event, run = git) {
  let base, head = "";
  if (event) {
    const pr = event.pull_request;
    if (![pr?.base?.sha, pr?.head?.sha].every((sha) => /^[a-f0-9]{40}$/.test(sha ?? ""))) throw new Error("Test diff needs valid PR base/head SHAs; rerun with a pull_request event.");
    // Checkout uses GitHub's synthetic merge tree. Comparing it to its first
    // parent excludes changes made only on main, even in a shallow checkout.
    const commit = run(["cat-file", "-p", "HEAD"]);
    const parents = [...commit.matchAll(/^parent ([a-f0-9]{40})$/gm)].map((m) => m[1]);
    if (parents.length !== 2 || parents[0] !== pr.base.sha || parents[1] !== pr.head.sha) throw new Error("Test diff needs the PR merge checkout matching the event; rerun the workflow.");
    base = parents[0]; head = "HEAD";
    try { run(["cat-file", "-e", `${base}^{commit}`]); }
    catch { run(["fetch", "--no-tags", "--depth=1", "origin", base]); }
  } else base = run(["merge-base", "origin/main", "HEAD"]).trim();
  if (!/^[a-f0-9]{40}$/.test(base)) throw new Error("Test diff needs origin/main; run git fetch origin main and retry.");
  const range = head ? [base, head] : [base];
  const names = run(["diff", "--no-ext-diff", "--no-textconv", "--no-renames", "--name-only", "-z", ...range, "--"]).split("\0").filter(Boolean);
  return names.filter((name) => testFile(name) || expectedFile(name)).map((filename) => ({
    filename, patch: run(["diff", "--no-ext-diff", "--no-textconv", "--no-renames", "--unified=0", ...range, "--", filename]),
  }));
}
