import assert from "node:assert/strict";
import { mkdtemp, readFile, readdir, rm, writeFile, cp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { test } from "node:test";
import { anchors, links } from "./doc-links.mjs";
import { build } from "../website/build.mjs";

const source = new URL("../website/", import.meta.url);
test("public build has four canonical routes, local assets, safe headers and no stale pages", async () => {
  const dir = await mkdtemp(join(tmpdir(), "ibis-website-"));
  try {
    await writeFile(join(dir, "stale.html"), "stale");
    await build(dir);
    const keys = await import(pathToFileURL(join(dir, "keys.js")));
    assert.equal(keys.keyLabel(keys.keysFor("revealSource", true), true), "⌥⌘O");
    assert.equal(keys.keyLabel(keys.keysFor("revealSource", false), false), "Ctrl+Shift+O");
    const paths = ["/", "/get/", "/features/", "/cli/"];
    const sitemap = await readFile(join(dir, "sitemap.xml"), "utf8");
    assert.deepEqual([...sitemap.matchAll(/<loc>(.*?)<\/loc>/g)].map(m => m[1]), paths.map(p => `https://ibiscite.org${p}`));
    assert.doesNotMatch(sitemap, /lastmod|workers.dev|#/);
    for (const path of [...paths, "/404.html"]) {
      const html = await readFile(join(dir, path === "/404.html" ? "404.html" : `${path}index.html`), "utf8");
      assert.match(html, new RegExp(`rel="canonical" href="https://ibiscite.org${path}"`));
      assert.doesNotMatch(html, /<script(?![^>]*src=)|style=|<style|googletagmanager|cloudflareinsights/);
      for (const [, asset] of html.matchAll(/(?:src|href)="(\/(?:assets\/[^"#]+|[^"/]+\.(?:css|js)))"/g)) await readFile(join(dir, asset));
      for (const [, target] of html.matchAll(/href="#([^"\s]+)"/g)) assert.ok(html.includes(`id="${target}"`), target);
      for (const [, reference] of html.matchAll(/(?:src|href|srcset)="([^"\s]+)"/g)) {
        const url = new URL(reference, `https://ibiscite.org${path}`);
        if (url.origin !== "https://ibiscite.org") {
          if (path === "/cli/" && url.protocol === "cursor:") {
            assert.equal(url.hostname, "anysphere.cursor-deeplink");
            assert.equal(url.pathname, "/mcp/install");
            continue;
          }
          if (path === "/cli/" && url.protocol === "vscode:") {
            assert.equal(url.pathname, "mcp/install");
            continue;
          }
          assert.equal(url.protocol, "https:", reference);
          // GitHub source links can be checked against this checkout offline.
          const file = url.pathname.match(/^\/ibiscite\/ibis\/blob\/main\/(.+)$/)?.[1];
          if (file) await readFile(new URL(`../${file}`, import.meta.url));
          continue;
        }
        const file = join(dir, url.pathname.endsWith("/") ? `${url.pathname}index.html` : url.pathname);
        const target = await readFile(file, "utf8");
        if (url.hash) assert.ok(target.includes(`id="${decodeURIComponent(url.hash.slice(1))}"`), `${path}: ${reference}`);
      }
    }
    const headers = await readFile(join(dir, "_headers"), "utf8");
    assert.match(headers, /^\/\*\n  Content-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'/);
    assert.match(headers, /X-Robots-Tag: noindex/);
    for (const path of ["setup", "setup.md"]) {
      assert.match(headers, new RegExp(`^/${path.replaceAll(".", "\\.")}\\n  Content-Type: text/markdown; charset=utf-8$`, "m"));
    }
    assert.match(headers, /^\/llms\.txt\n  Content-Type: text\/plain; charset=utf-8$/m);
    const original = await readFile(new URL("../docs/agents.md", import.meta.url), "utf8");
    const guide = await readFile(join(dir, "setup"), "utf8");
    assert.equal(guide, original.replaceAll("](cli.md#", "](https://github.com/ibiscite/ibis/blob/main/docs/cli.md#"));
    assert.equal(await readFile(join(dir, "setup.md"), "utf8"), guide);
    assert.deepEqual(links(guide), [], "the published guide has no relative links");
    assert.match(guide, /^Official address of this guide: https:\/\/ibiscite.org\/setup$/m);
    const llms = await readFile(join(dir, "llms.txt"), "utf8");
    assert.match(llms, /^# Ibis\n\n> [^\n]+\n\n## /);
    const targets = [...llms.matchAll(/^- \[[^\]]+\]\(([^)]+)\)/gm)].map(m => m[1]);
    assert.deepEqual(targets, [
      "https://ibiscite.org/setup",
      "https://github.com/ibiscite/ibis/blob/main/docs/cli.md",
      "https://github.com/ibiscite/ibis/blob/main/docs/cli.md#ai-agents-mcp",
      "https://github.com/ibiscite/ibis/blob/main/PRIVACY.md",
      "https://github.com/ibiscite/ibis/blob/main/TRADEMARKS.md",
    ]);
    for (const target of targets) {
      const url = new URL(target);
      const path = url.origin === "https://ibiscite.org" ? join(dir, url.pathname)
        : new URL(`../${url.pathname.slice("/ibiscite/ibis/blob/main/".length)}`, import.meta.url);
      const text = await readFile(path, "utf8");
      if (url.hash) assert.ok(anchors(text).has(url.hash.slice(1)), target);
    }
    assert.match(await readFile(join(dir, "404.html"), "utf8"), /noindex/);
    assert.equal(await readFile(join(dir, "tokens.css"), "utf8"), await readFile(new URL("../apps/desktop/src/tokens.css", import.meta.url), "utf8"));
    assert.deepEqual((await readdir(dir)).sort(), ["404.html", "LICENSE-phosphor", "_headers", "arrow-up-right.svg", "assets", "cli", "demo-model.mjs", "demo.css", "demo.js", "features", "get", "icon-art.js", "icon.css", "index.html", "keys.js", "llms.txt", "picker-tokens.css", "platform.js", "robots.txt", "setup", "setup.md", "site.js", "sitemap.xml", "style.css", "tokens.css"]);
  } finally { await rm(dir, { recursive: true, force: true }); }
});

test("a failed build preserves the last published pages and refuses replacing source", async () => {
  const dir = await mkdtemp(join(tmpdir(), "ibis-build-"));
  try {
    const fixture = join(dir, "source"), output = join(dir, "public");
    await cp(source, fixture, { recursive: true, filter: path => !path.includes("/dist") });
    await build(output, fixture);
    const good = await readFile(join(output, "index.html"), "utf8");
    await rm(join(fixture, "index.html"));
    await assert.rejects(build(output, fixture));
    assert.equal(await readFile(join(output, "index.html"), "utf8"), good);
    await assert.rejects(build(fixture, fixture), /source/);
  } finally { await rm(dir, { recursive: true, force: true }); }
});
