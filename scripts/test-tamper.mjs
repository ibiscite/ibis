// Heuristic guard: legitimate test rewrites need a visible explanation, not a ban.
import { existsSync, readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";

const testPath = (name) => /(?:^|\/)(?:__tests__|tests|test)[\/]|(?:^|\/)test_[^/]+$|(?:[._](?:test|spec)\.[^/]+$)/.test(name);
const testFile = (name) => testPath(name) || name.endsWith(".rs");
const expectedFile = (name) => /(?:^|\/)(?:fixtures|proptest-regressions|snapshots)\/|\.snap(?:\.new)?$/.test(name);

function rustTestLines(source = "") {
  const tokens = /r(#{0,255})"[\s\S]*?"\1|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])'|\/\/[^\n]*|\/\*/g;
  const chunks = [];
  let previous = 0, token;
  while ((token = tokens.exec(source))) {
    let end = tokens.lastIndex;
    if (token[0] === "/*") {
      const comments = /\/\*|\*\//g; comments.lastIndex = end;
      let depth = 1, next;
      while (depth && (next = comments.exec(source))) { depth += next[0] === "/*" ? 1 : -1; end = comments.lastIndex; }
      if (depth) end = source.length;
      tokens.lastIndex = end;
    }
    chunks.push(source.slice(previous, token.index), source.slice(token.index, end).replace(/[^\n]/g, " "));
    previous = end;
  }
  const masked = chunks.join("") + source.slice(previous);
  const lines = new Set();
  for (const match of masked.matchAll(/#\s*\[\s*(?:cfg\s*\(\s*test\s*\)|(?:\w+::)?test)\s*\][^{;]*\{/g)) {
    let end = match.index + match[0].length, depth = 1;
    while (end < masked.length && depth) { if (masked[end] === "{") depth++; if (masked[end] === "}") depth--; end++; }
    const startLine = masked.slice(0, match.index).split("\n").length;
    const endLine = startLine + masked.slice(match.index, end).split("\n").length - 1;
    for (let line = startLine; line <= endLine; line++) lines.add(line);
  }
  return lines;
}

export function testTamper(files, body = "") {
  const visible = body.replace(/<!--[\s\S]*?(?:-->|$)|```[\s\S]*?(?:```|$)|~~~[\s\S]*?(?:~~~|$)/g, "");
  if ([...visible.matchAll(/^ {0,3}Test changes:[ \t]*(.*)$/gmi)].some((match) => /\p{L}/u.test(match[1].replace(/<[^>]*(?:>|$)/g, "")))) return [];
  const flagged = files.some(({ filename, patch, before, after }) => {
    const name = filename.replaceAll("\\", "/");
    if (expectedFile(name)) return true;
    if (!testFile(name)) return false;
    const inline = name.endsWith(".rs") && !testPath(name);
    const oldTests = inline ? rustTestLines(before) : null, newTests = inline ? rustTestLines(after) : null;
    const removedLines = [], addedLines = [];
    let oldLine = 1, newLine = 1;
    for (const line of patch.split(/\r?\n/)) {
      const hunk = line.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
      if (hunk) { oldLine = Number(hunk[1]); newLine = Number(hunk[2]); continue; }
      if (/^(?:---|\+\+\+|diff |index |\\)/.test(line)) continue;
      if (line.startsWith("-")) { if (!inline || oldTests.has(oldLine)) removedLines.push(line.slice(1)); oldLine++; }
      else if (line.startsWith("+")) { if (!inline || newTests.has(newLine) || /#\s*\[\s*ignore\b/.test(line)) addedLines.push(line.slice(1)); newLine++; }
      else if (line.startsWith(" ")) { oldLine++; newLine++; }
    }
    const removed = removedLines.join("\n"), added = addedLines.join("\n");
    return /\b(?:assert(?:_\w+)?!?\s*(?:[.(]|[ \t]+)|expect\s*[.(]|self\.assert\w*\s*\()/.test(removed)
      || /^\s*\.\s*(?:not\s*\.\s*)?(?:to[A-Z]\w*|rejects|resolves)\b/m.test(removed)
      || /\b(?:skip|only|todo)\s*:\s*(?!false\b)[^,}\s]/.test(added)
      || /#\s*\[\s*ignore\b|\.\s*(?:skip|only|todo)\b|\btodo\s*!\s*\(/.test(added);
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

export function testDiff(event, run = git, merge = process.env.GITHUB_SHA ?? event?.pull_request?.merge_commit_sha) {
  let base, head = "";
  if (event) {
    const pr = event.pull_request;
    if (![pr?.base?.sha, pr?.head?.sha].every((sha) => /^[a-f0-9]{40}$/.test(sha ?? ""))) throw new Error("Test diff needs valid PR base/head SHAs; rerun with a pull_request event.");
    // Checkout uses GitHub's synthetic merge tree. Comparing it to its first
    // parent excludes changes made only on main, even in a shallow checkout.
    const commit = run(["cat-file", "-p", "HEAD"]);
    const parents = [...commit.matchAll(/^parent ([a-f0-9]{40})$/gm)].map((m) => m[1]);
    // The event's base SHA can lag behind GitHub's regenerated merge ref.
    if (!/^[a-f0-9]{40}$/.test(merge ?? "") || run(["rev-parse", "HEAD"]).trim() !== merge || parents.length !== 2 || parents[1] !== pr.head.sha) throw new Error("Test diff needs the PR merge checkout matching the event; rerun the workflow.");
    base = parents[0]; head = "HEAD";
    try { run(["cat-file", "-e", `${base}^{commit}`]); }
    catch { run(["fetch", "--no-tags", "--depth=1", "origin", base]); }
  } else base = run(["merge-base", "origin/main", "HEAD"]).trim();
  if (!/^[a-f0-9]{40}$/.test(base)) throw new Error("Test diff needs origin/main; run git fetch origin main and retry.");
  const range = head ? [base, head] : [base];
  const names = run(["diff", "--no-ext-diff", "--no-textconv", "--no-renames", "--name-only", "-z", ...range, "--"]).split("\0").filter(Boolean);
  return names.filter((name) => testFile(name) || expectedFile(name)).map((filename) => {
    const file = { filename, patch: run(["diff", "--no-ext-diff", "--no-textconv", "--no-renames", "--unified=0", ...range, "--", filename]) };
    if (filename.endsWith(".rs") && !testPath(filename)) {
      const at = (commit) => run(["ls-tree", "--name-only", "-z", commit, "--", filename]) ? run(["show", `${commit}:${filename}`]) : "";
      file.before = at(base);
      file.after = head ? at(head) : existsSync(filename) ? readFileSync(filename, "utf8") : "";
    }
    return file;
  });
}
