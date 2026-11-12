// Check the Markdown people can read in this checkout, including local renames.
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, posix } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

// Vendored files are upstream's, with links into upstream's tree. Include new
// documents and omit deleted sources; links to missing targets still fail.
export function documents() {
  return execFileSync("git", ["ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", "*.md"], { cwd: root, encoding: "utf8" })
    .split("\0").filter((path) => path && !path.split("/").includes("vendor") && existsSync(join(root, path)));
}

// What a reader sees, without code: fenced blocks and inline code spans.
const prose = (text) => text.replace(/^(```|~~~)[^\n]*\n[\s\S]*?^\1[^\n]*$/gm, "").replace(/`[^`\n]*`/g, "");

// GitHub's heading anchors: rendered text, lower case, punctuation dropped,
// spaces as hyphens, repeats numbered.
export function anchors(text) {
  const seen = new Map();
  const found = new Set();
  for (const [, heading] of prose(text).matchAll(/^#{1,6}[ \t]+(.+?)[ \t#]*$/gm)) {
    const words = heading.replace(/\[([^\]]*)\]\([^)]*\)/g, "$1").replace(/[*`]/g, "");
    const base = words.toLowerCase().replace(/[^\p{L}\p{N}\p{M} _-]/gu, "").replace(/ /g, "-");
    const count = seen.get(base) ?? 0;
    seen.set(base, count + 1);
    found.add(count ? `${base}-${count}` : base);
  }
  for (const [, id] of text.matchAll(/<a\s+(?:id|name)="([^"]+)"/g)) found.add(id);
  return found;
}

export function links(text) {
  return [...prose(text).matchAll(/\]\(<?([^)\s>]+)>?(?:\s+"[^"]*")?\)/g)].map(([, target]) => target)
    .filter((target) => !/^[a-z][a-z0-9+.-]*:/i.test(target));
}

export function brokenLinks(paths, read = (path) => readFileSync(join(root, path), "utf8"), exists = (path) => existsSync(join(root, path))) {
  const broken = [];
  const cache = new Map();
  const headings = (path) => cache.get(path) ?? cache.set(path, anchors(read(path))).get(path);
  for (const path of paths) {
    for (const target of links(read(path))) {
      const [file, anchor] = target.split("#");
      // Repository paths use "/" on every system, as Git writes them.
      const resolved = file ? posix.normalize(posix.join(posix.dirname(path), decodeURIComponent(file))) : path;
      if (resolved.startsWith("..") || !exists(resolved)) broken.push(`${path}: ${target} (no such file)`);
      else if (anchor && resolved.endsWith(".md") && !headings(resolved).has(decodeURIComponent(anchor).toLowerCase())) broken.push(`${path}: ${target} (no such heading)`);
    }
  }
  return broken;
}
