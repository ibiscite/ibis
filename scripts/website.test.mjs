import assert from "node:assert/strict";
import { mkdtemp, readFile, readdir, rm, writeFile, cp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { build } from "../website/build.mjs";

const source = new URL("../website/", import.meta.url);
test("public build has four canonical routes, local assets, safe headers and no stale pages", async () => {
  const dir = await mkdtemp(join(tmpdir(), "ibis-website-"));
  try {
    await writeFile(join(dir, "stale.html"), "stale");
    await build(dir);
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
    }
    const headers = await readFile(join(dir, "_headers"), "utf8");
    assert.match(headers, /^\/\*\n  Content-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'/);
    assert.match(headers, /X-Robots-Tag: noindex/);
    assert.match(await readFile(join(dir, "404.html"), "utf8"), /noindex/);
    assert.equal(await readFile(join(dir, "tokens.css"), "utf8"), await readFile(new URL("../apps/desktop/src/tokens.css", import.meta.url), "utf8"));
    assert.deepEqual((await readdir(dir)).sort(), ["404.html", "LICENSE-lucide", "_headers", "arrow-up-right.svg", "assets", "cli", "demo-model.mjs", "demo.css", "demo.js", "features", "get", "icon-paths.js", "icon.css", "index.html", "keys.js", "picker-tokens.css", "robots.txt", "site.js", "sitemap.xml", "style.css", "tokens.css"]);
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
