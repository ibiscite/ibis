// Static reference validation; documentation commands are never executed.
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { posix } from "node:path";
import { pathToFileURL } from "node:url";

const baselinePath = "scripts/docs-drift-baseline.json";
const normalize = (value) => value.replaceAll("\\", "/");
const placeholder = (value) => /[<$…]|\.\.\.|^[a-z][a-z+.-]*:|^\//i.test(value);
function glob(pattern, value) {
  const tokens = pattern.match(/\*{2,}|\*|\?|[^*?]/gu) ?? [], chars = [...value];
  let row = new Uint8Array(chars.length + 1); row[0] = 1;
  for (const token of tokens) {
    const next = new Uint8Array(chars.length + 1), star = token.startsWith("*");
    if (star) next[0] = row[0];
    for (let i = 1; i <= chars.length; i++) next[i] = star
      ? Number(Boolean(row[i] || next[i - 1] && (token.length > 1 || chars[i - 1] !== "/")))
      : Number(Boolean(row[i - 1] && (token === "?" ? chars[i - 1] !== "/" : token === chars[i - 1])));
    row = next;
  }
  return Boolean(row[chars.length]);
}

function* codeSpans(text) {
  const runs = [];
  let cursor = 0, lineStart = 0, fence = 0, start;
  while ((start = text.indexOf("`", cursor)) >= 0) {
    const newline = text.slice(cursor, start).lastIndexOf("\n");
    if (newline >= 0) lineStart = cursor + newline + 1;
    let end = start + 1;
    while (text[end] === "`") end++;
    const length = end - start;
    if (length >= 3 && start - lineStart <= 3 && /^ *$/.test(text.slice(lineStart, start))) {
      const lineEnd = text.indexOf("\n", end), tail = text.slice(end, lineEnd < 0 ? text.length : lineEnd);
      if (fence && length >= fence && /^[ \t]*$/.test(tail)) fence = 0;
      else if (!fence && !tail.includes("`")) fence = length;
      else if (!fence) runs.push({ start, end, length });
    } else if (!fence) runs.push({ start, end, length });
    cursor = end;
  }
  const next = new Map(), pairs = new Array(runs.length).fill(-1);
  for (let i = runs.length - 1; i >= 0; i--) { pairs[i] = next.get(runs[i].length) ?? -1; next.set(runs[i].length, i); }
  for (let i = 0; i < runs.length; i++) {
    if (pairs[i] < 0) continue;
    yield text.slice(runs[i].end, runs[pairs[i]].start).replace(/\r?\n/g, " ").trim();
    i = pairs[i];
  }
}

function* inlineLinks(text) {
  for (const line of text.split(/\r?\n/)) {
    let opening = -1;
    for (let i = 0; i < line.length; i++) {
      if (line[i] === "[") opening = i;
      if (opening < 0 || line[i] !== "]" || line[i + 1] !== "(") continue;
      let at = i + 2;
      while (/[ \t]/.test(line[at] ?? "")) at++;
      let end = at;
      if (line[at] === "<") { end = line.indexOf(">", at + 1); if (end < 0) break; end++; }
      else while (end < line.length && !/[ \t)]/.test(line[end])) end++;
      const close = line.indexOf(")", end);
      if (close < 0) break;
      yield line.slice(at, end).replace(/^<|>$/g, "");
      i = close; opening = -1;
    }
  }
}

export function docProblems(doc, text, files, crates) {
  doc = normalize(doc);
  text = text.replace(/\r\n?/g, "\n");
  text = text.replace(/\\\r?\n[ \t]*/g, " ");
  const paths = new Set(files.map(normalize));
  for (const file of [...paths]) {
    let parent = posix.dirname(file);
    while (parent !== ".") { paths.add(parent); parent = posix.dirname(parent); }
  }
  const roots = new Set([...paths].map((name) => name.split("/")[0]));
  const problems = [];
  let budget = 10000000;
  const matches = (pattern, value) => {
    budget -= pattern.length * value.length;
    if (budget < 0) throw new Error(`${doc}: glob matching budget exceeded; replace broad globs with short, concrete repository paths.`);
    return glob(pattern, value);
  };
  function exists(value, link) {
    value = normalize(value).replace(/\/$/, "");
    const candidates = link ? [posix.normalize(posix.join(posix.dirname(doc), value))] : [posix.normalize(value), posix.normalize(posix.join(posix.dirname(doc), value))];
    if (candidates.some((candidate) => paths.has(candidate) || /[*?]/.test(candidate) && [...paths].some((path) => matches(candidate, path)))) return true;
    // Component-relative prose (e.g. picker/dialogs.rs) and bare filenames.
    return !link && !roots.has(value.split("/")[0]) && !value.startsWith(".") && [...paths].some((path) => path.endsWith("/" + value) || /[*?]/.test(value) && matches("**/" + value, path) || /\/(?:ui|features)\//.test(path) && path.replace(/\.[^.]+$/, "").endsWith("/" + value));
  }
  function path(value, kind, link = false) {
    if (placeholder(value) || value.startsWith("#") || /^(?:(?:origin|claude|dependabot)\/|(?:feat|fix|perf|docs|test|ci|build|refactor|chore|revert)\/\d+-)/.test(value) || /^[~%]|^[A-Z_]+=/.test(value)) return;
    let name;
    try { name = decodeURIComponent(value.split(link ? /[?#]/ : /#/)[0]); }
    catch { problems.push(`${doc}: ${kind} ${value}`); return; }
    if (name && !exists(name, link)) problems.push(`${doc}: ${kind} ${value}`);
  }
  for (const span of codeSpans(text)) {
    const value = normalize(span);
    if (/^(?:node|cargo|pnpm|python\d*)\s/.test(value) || /^[-#]|\s--/.test(value) || value.startsWith(".local/")) continue;
    if (/\s/.test(value) && (!/^(?:\.\.?\/|[^\s/]+\/)/.test(value) || /[=;()"']/.test(value))) continue;
    if (/\//.test(value) || /^[\w.-]+\.(?:md|[cm]?js|rs|tsx?|css|json|toml|ya?ml|py|sh|ps1)$/.test(value)) path(value, "path");
  }
  for (const link of inlineLinks(text)) path(link, "link", true);
  for (const match of text.matchAll(/^[ \t]*\[[^\]\r\n]+\]:[ \t]*(\S+)/gm)) path(match[1].replace(/^<|>$/g, ""), "link", true);
  for (const match of text.matchAll(/\bnode\s+(?:"(scripts[\\/][^"\r\n]+)"|'(scripts[\\/][^'\r\n]+)'|(scripts[\\/][^\s`;'"|)]+))/g)) path(normalize(match[1] ?? match[2] ?? match[3]), "node script");
  for (const command of text.matchAll(/\bcargo(?:\s+\+[^\s`]+)?\s+([^\r\n`]+)/g)) {
    for (const match of command[1].split(/\s--(?:\s|$)/)[0].matchAll(/(?:^|\s)(?:-p(?:=|\s+)?|--package(?:=|\s+))(['"]?)([^\s`;'"|)]+)\1/g)) {
      if (!placeholder(match[2]) && !crates.includes(match[2])) problems.push(`${doc}: cargo package ${match[2]}`);
    }
  }
  return problems;
}

export function baselineErrors(problems, baseline) {
  const validKey = (key) => /^(?:AGENTS\.md|CONTRIBUTING\.md|docs\/[^/]+\.md): (?:path|link|node script|cargo package) .+$/.test(key) && !/[\x00-\x1f]/.test(key);
  if (!baseline || typeof baseline !== "object" || Array.isArray(baseline) || ![Object.prototype, null].includes(Object.getPrototypeOf(baseline))
    || Object.entries(baseline).some(([key, count]) => !validKey(key) || !Number.isSafeInteger(count) || count < 1)) return ["Docs-drift baseline needs valid reference keys and positive safe-integer counts; fix scripts/docs-drift-baseline.json."];
  const counts = {};
  for (const problem of problems) counts[problem] = (counts[problem] ?? 0) + 1;
  return [...Object.entries(counts).filter(([key, n]) => n > (baseline[key] ?? 0)).map(([key]) => `${key}: correct this reference to an existing repository path or package.`),
    ...Object.keys(baseline).filter((key) => (counts[key] ?? 0) < baseline[key]).map((key) => `${key}: remove or lower the stale docs-drift baseline entry.`)];
}

function main() {
  const result = spawnSync("git", ["-c", "core.fsmonitor=false", "ls-files", "-z"], { encoding: "utf8", timeout: 30000, killSignal: "SIGKILL", maxBuffer: 32 * 1024 * 1024 });
  if (result.status !== 0 || result.error) throw new Error("Docs drift needs repository file metadata; run from the repository root with Git installed.");
  const files = result.stdout.split("\0").filter((name) => name && existsSync(name));
  const crates = files.filter((name) => /(?:^|\/)Cargo\.toml$/.test(name)).flatMap((name) => {
    const section = readFileSync(name, "utf8").match(/^\[package\]([\s\S]*?)(?=^\[|$(?![\s\S]))/m)?.[1] ?? "";
    return [...section.matchAll(/^name\s*=\s*"([^"]+)"/gm)].map((match) => match[1]);
  });
  const problems = files.filter((name) => /^(?:AGENTS\.md|CONTRIBUTING\.md|docs\/[^/]+\.md)$/.test(name)).flatMap((name) => docProblems(name, readFileSync(name, "utf8"), files, crates));
  const baseline = JSON.parse(readFileSync(baselinePath, "utf8"));
  const errors = baselineErrors(problems, baseline);
  if (errors.length) { console.error(errors.join("\n")); process.exitCode = 1; }
  else console.log(`Docs drift passed (${problems.length} recorded references).`);
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try { main(); } catch (error) { console.error(error.message); process.exitCode = 1; }
}
