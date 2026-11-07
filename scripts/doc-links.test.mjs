// Every relative link in the repository's Markdown reaches a file, and every
// #anchor into Markdown reaches a heading there, so splitting a document can
// never leave a reader at a dead end.
import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

// Vendored files are upstream's, with links into upstream's tree.
export function documents() {
  return execFileSync("git", ["ls-files", "-z", "*.md"], { cwd: root, encoding: "utf8" })
    .split("\0").filter((path) => path && !path.split("/").includes("vendor"));
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
      const resolved = file ? normalize(join(dirname(path), decodeURIComponent(file))) : path;
      if (resolved.startsWith("..") || !exists(resolved)) broken.push(`${path}: ${target} (no such file)`);
      else if (anchor && resolved.endsWith(".md") && !headings(resolved).has(decodeURIComponent(anchor).toLowerCase())) broken.push(`${path}: ${target} (no such heading)`);
    }
  }
  return broken;
}

test("every relative link in the repository's Markdown reaches its file and heading", () => {
  assert.deepEqual(brokenLinks(documents()), []);
});

test("a moved file, a renamed heading and a link outside the repository are each reported", () => {
  const files = {
    "docs/a.md": "# A\n\n## Same\n\n## Same\n\nSee [b](b.md#two-words), [gone](gone.md), [top](#a), [second](#same-1), [old](#old) and [up](../../x.md).\n\n```\n[not a link](nowhere.md)\n```\n",
    "docs/b.md": "# B\n\n## Two **words**!\n",
  };
  assert.deepEqual(brokenLinks(["docs/a.md"], (path) => files[path], (path) => path in files), [
    "docs/a.md: gone.md (no such file)",
    "docs/a.md: #old (no such heading)",
    "docs/a.md: ../../x.md (no such file)",
  ]);
});
