import assert from "node:assert/strict";
import { mkdtemp, readFile, readdir, rm, writeFile, mkdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { build, scopePickerTokens } from "../website/build.mjs";

test("picker token changes reach the website without a second palette", async () => {
  const tokens = await readFile(new URL("../apps/desktop/src/tokens.css", import.meta.url), "utf8");
  const scoped = scopePickerTokens(tokens);
  assert.equal(scoped.replaceAll(":where([data-app-tokens])", ":root"), tokens);
  const changed = tokens.replace("--row-comfortable: 3.625rem", "--row-comfortable: 4.5rem");
  assert.notEqual(changed, tokens);
  assert.match(scopePickerTokens(changed), /--row-comfortable: 4.5rem/);
  assert.match(scoped, /:where\(\[data-app-tokens\]\):not\(\.notice-window\)/);
  assert.match(scoped, /@media \(forced-colors: active\)/);
});

test("public build copies canonical tokens and only allowlisted assets", async () => {
  const dir = await mkdtemp(join(tmpdir(), "ibis-website-"));
  try {
    const source = join(dir, "source"), output = join(dir, "public");
    await mkdir(source); await mkdir(output);
    for (const name of ["index.html", "404.html", "style.css", "site.js", "_headers", "demo.js", "demo.css", "demo-model.mjs", "robots.txt", "sitemap.xml"]) await writeFile(join(source, name), "synthetic");
    await writeFile(join(source, "private.txt"), "must not publish");
    await writeFile(join(output, "stale.txt"), "stale");
    await build(output, source);
    assert.deepEqual((await readdir(output)).sort(), ["404.html", "_headers", "assets", "demo-model.mjs", "demo.css", "demo.js", "icon-paths.js", "index.html", "keys.js", "picker-tokens.css", "robots.txt", "site.js", "sitemap.xml", "style.css", "tokens.css"]);
    assert.equal(await readFile(join(output, "tokens.css"), "utf8"), await readFile(new URL("../apps/desktop/src/tokens.css", import.meta.url), "utf8"));
    assert.equal(await readFile(join(output, "picker-tokens.css"), "utf8"), scopePickerTokens(await readFile(join(output, "tokens.css"), "utf8")));
    const manifest = JSON.parse(await readFile(join(output, "assets/site.webmanifest"), "utf8"));
    for (const icon of manifest.icons) await readFile(join(output, "assets", icon.src));
    for (const file of ["favicon.svg", "png/og-image.png"]) assert.deepEqual(await readFile(join(output, "assets", file)), await readFile(new URL(`../brand/exports/${file}`, import.meta.url)));
    await rm(join(source, "index.html"));
    await assert.rejects(build(output, source));
    assert.equal(await readFile(join(output, "index.html"), "utf8"), "synthetic", "failed build preserves last good output");
    await assert.rejects(build(source, source), /source/);
  } finally { await rm(dir, { recursive: true, force: true }); }
});


test("Cloudflare serves a self-only policy and bounded caches without Apache", async () => {
  const headers = await readFile(new URL("../website/_headers", import.meta.url), "utf8");
  assert.match(headers, /default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'/);
  assert.doesNotMatch(headers.replace(/^https:\/\/:version.*$/m, ""), /https?:|data:|unsafe-eval/);
  assert.match(headers, /Cache-Control: no-cache/);
  assert.match(headers, /Cache-Control: public, max-age=3600, must-revalidate/);
  assert.match(headers, /https:\/\/:version\.:subdomain\.workers\.dev\/\*\n  X-Robots-Tag: noindex/, "preview links stay out of search results");
  assert.match(headers, /! Content-Security-Policy\n  Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; sandbox/);
  const config = JSON.parse((await readFile(new URL("../wrangler.jsonc", import.meta.url), "utf8")).replace(/^\s*\/\/.*$/gm, ""));
  assert.deepEqual(config.assets, { directory: "./website/dist", not_found_handling: "404-page" });
  assert.equal(config.main, undefined, "static assets need no Worker code");
  assert.deepEqual(config.previews, {}, "pull request previews use the top-level static assets");
  const source = await readdir(new URL("../website/", import.meta.url));
  assert.ok(!source.includes(".htaccess") && !source.includes("deploy.mjs"));
});


test("search discovery publishes only the canonical homepage without tracking", async () => {
  const dir = await mkdtemp(join(tmpdir(), "ibis-seo-"));
  try {
    const output = join(dir, "public");
    await build(output);
    const sitemap = await readFile(join(output, "sitemap.xml"), "utf8");
    assert.match(sitemap, /<urlset xmlns="http:\/\/www.sitemaps.org\/schemas\/sitemap\/0.9">/);
    assert.deepEqual([...sitemap.matchAll(/<loc>(.*?)<\/loc>/g)].map(match => match[1]), ["https://ibiscite.org/"]);
    assert.doesNotMatch(sitemap, /lastmod|workers\.dev|#|priority|changefreq/);
    const robots = await readFile(join(output, "robots.txt"), "utf8");
    assert.match(robots, /User-agent: \*\nAllow: \/\n/);
    assert.match(robots, /Sitemap: https:\/\/ibiscite.org\/sitemap.xml/);
    const html = await readFile(join(output, "index.html"), "utf8");
    assert.match(html, /itemscope itemtype="https:\/\/schema.org\/WebSite"/);
    assert.match(html, /itemprop="mainEntity" itemscope itemtype="https:\/\/schema.org\/SoftwareApplication"/);
    assert.match(html, /itemprop="isAccessibleForFree" content="true"/);
    assert.match(html, /itemprop="operatingSystem" content="Windows, macOS, Linux"/);
    assert.doesNotMatch(html, /aggregateRating|ratingValue|googletagmanager|google-analytics|clarity\.ms|cloudflareinsights/);
    assert.ok([...html.matchAll(/<script[^>]+src="([^"]+)"/g)].every(([, src]) => !src.includes(":")), "scripts remain local");
  } finally { await rm(dir, { recursive: true, force: true }); }
});

test("built homepage keeps a visible picker example and its local interactive assets", async () => {
  const dir = await mkdtemp(join(tmpdir(), "ibis-demo-build-"));
  try {
    await build(dir);
    const html = await readFile(join(dir, "index.html"), "utf8");
    assert.ok(/<figure[^>]*data-picker/.test(html), "the public build must show the picker in the second step");
    assert.match(html, /Try it with made-up references/);
    assert.match(html, /Copy as Pandoc/);
    assert.match(html, /<link rel="stylesheet" href="picker-tokens.css">/);
    assert.match(html, /class="picker-panel" data-app-tokens data-platform="macos"/);
    assert.match(html, /class="copied-notice notice-window" data-app-tokens/);
    assert.match(html, /class="picker-mark" aria-hidden="true"/);
    const css = await readFile(join(dir, "demo.css"), "utf8");
    assert.doesNotMatch(css, /#[\da-f]{3,8}\b|(?:rgb|hsl|oklch)\(/i, "pictures never maintain their own colour values");
    assert.doesNotMatch(css, /prefers-color-scheme/, "app pictures inherit color-scheme through the shared role tokens");
    for (const [, asset] of css.matchAll(/url\("(assets\/[^"\s]+)"\)/g)) await readFile(join(dir, asset));
    for (const name of ["demo.js", "demo.css", "demo-model.mjs", "keys.js", "icon-paths.js"]) {
      assert.ok((await readFile(join(dir, name), "utf8")).length > 0, name);
    }
    assert.match(await readFile(join(dir, "site.js"), "utf8"), /import\("\.\/demo\.js"\)/);
  } finally { await rm(dir, { recursive: true, force: true }); }
});
