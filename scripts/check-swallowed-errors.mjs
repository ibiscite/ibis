// Use the parser already pinned by Vite; no new dependency or parser rules.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";
const desktopRequire = createRequire(new URL("../apps/desktop/package.json", import.meta.url));
const viteRequire = createRequire(desktopRequire.resolve("vite"));
const { parseSync } = await import(viteRequire.resolve("rolldown/utils"));
const baselinePath = "scripts/swallowed-errors-baseline.json";
const unwrap = (node) => { while (["ParenthesizedExpression", "TSAsExpression", "TSNonNullExpression", "TSSatisfiesExpression", "TSTypeAssertion", "TSInstantiationExpression"].includes(node?.type)) node = node.expression; return node; };
const empty = (body) => body?.type === "BlockStatement" && body.body.every((node) => node.type === "EmptyStatement" || node.type === "ReturnStatement" && node.argument == null);
const emptyVoid = (node) => {
  if (node?.type !== "UnaryExpression" || node.operator !== "void") return false;
  do { node = unwrap(node.argument); } while (node?.type === "UnaryExpression" && node.operator === "void");
  // Calling or awaiting a reporter still handles the error, even without a result.
  return ["Literal", "Identifier"].includes(node?.type);
};

export function catchProblems(path, source) {
  path = path.replaceAll("\\", "/");
  source = source.replace(/\r\n/g, "\n");
  const parsed = parseSync(path, source);
  if (parsed.errors.length) throw new Error(`${path}: fix source syntax before running the swallowed-error check.`);
  const starts = [0];
  for (let i = 0; i < source.length; i++) {
    const char = source.charCodeAt(i);
    if (char === 10 || char === 13 || char === 0x2028 || char === 0x2029) starts.push(i + 1);
  }
  const lineAt = (offset) => {
    let low = 0, high = starts.length;
    while (low < high) { const middle = (low + high) >>> 1; if (starts[middle] <= offset) low = middle + 1; else high = middle; }
    return low;
  };
  const explained = new Set(parsed.comments.filter((comment) => /\p{L}/u.test(comment.value.replace(/\b(?:TODO|FIXME|ignore[ds]?|intentional)\b/gi, ""))).map((comment) => lineAt(comment.start)));
  const problems = [], stack = [parsed.program];
  while (stack.length) {
    const node = stack.pop();
    if (!node || typeof node !== "object") continue;
    let position, callback;
    if (node.type === "CatchClause" && (empty(node.body) || node.param?.type === "Identifier" && /^_/.test(node.param.name))) position = node.start;
    const callee = node.type === "CallExpression" ? unwrap(node.callee) : null;
    if (callee?.type === "MemberExpression" && (callee.property.name === "catch" || callee.computed && callee.property.value === "catch")) {
      callback = unwrap(node.arguments[0]);
      if (["ArrowFunctionExpression", "FunctionExpression"].includes(callback?.type) && (empty(callback.body) || callback.body.type === "Identifier" && callback.body.name === "undefined" || emptyVoid(callback.body))) position = callee.property.start;
    }
    if (position != null && !explained.has(lineAt(position)) && !explained.has(lineAt(callback?.start ?? position))) {
      problems.push({ path, line: lineAt(position), key: `${path}:${createHash("sha256").update(source.slice(node.start, node.end)).digest("hex").slice(0, 16)}` });
    }
    for (const value of Object.values(node)) {
      if (Array.isArray(value)) { for (const child of value) stack.push(child); }
      else if (value?.type) stack.push(value);
    }
  }
  return problems.sort((a, b) => a.line - b.line);
}

export function catchBaselineErrors(problems, baseline) {
  const validKey = (key) => {
    const path = key.match(/^(apps\/desktop\/src\/.+\.[cm]?[jt]sx?):[a-f0-9]{16}$/)?.[1];
    return path && !/[\x00-\x1f\\]/.test(path) && path.split("/").every((part) => part && part !== "." && part !== "..");
  };
  if (!baseline || typeof baseline !== "object" || Array.isArray(baseline) || ![Object.prototype, null].includes(Object.getPrototypeOf(baseline))
    || Object.entries(baseline).some(([key, count]) => !validKey(key) || !Number.isSafeInteger(count) || count < 1)) return ["Swallowed-error baseline needs valid source/hash keys and positive safe-integer counts; fix scripts/swallowed-errors-baseline.json."];
  const counts = new Map();
  for (const problem of problems) counts.set(problem.key, (counts.get(problem.key) ?? 0) + 1);
  return [...problems.filter((problem) => counts.get(problem.key) > (baseline[problem.key] ?? 0)).map((problem) => `${problem.path}:${problem.line}: handle the error, or add a comment on the catch line explaining why ignoring it is safe.`),
    ...Object.keys(baseline).filter((key) => (counts.get(key) ?? 0) < baseline[key]).map((key) => `${key}: remove or lower this stale swallowed-error baseline entry.`)];
}

function main() {
  const result = spawnSync("git", ["-c", "core.fsmonitor=false", "ls-files", "-z", "--", "apps/desktop/src"], { encoding: "utf8", timeout: 30000, killSignal: "SIGKILL", maxBuffer: 32 * 1024 * 1024 });
  if (result.status !== 0 || result.error) throw new Error("Swallowed-error check needs Git metadata; run from the repository root with Git installed.");
  const problems = result.stdout.split("\0").filter((name) => /\.[cm]?[jt]sx?$/.test(name)).flatMap((name) => catchProblems(name, readFileSync(name, "utf8")));
  const errors = catchBaselineErrors(problems, JSON.parse(readFileSync(baselinePath, "utf8")));
  if (errors.length) { console.error(errors.join("\n")); process.exitCode = 1; }
  else console.log(`Swallowed-error check passed (${problems.length} existing catches recorded).`);
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try { main(); } catch (error) { console.error(error.message); process.exitCode = 1; }
}
