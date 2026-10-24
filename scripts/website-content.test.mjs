import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import { content } from "../website/content.mjs";

test("website headline and planned features come from the canonical sources", async () => {
  const html = await content(await readFile(new URL("../website/index.html", import.meta.url), "utf8"));
  const brand = JSON.parse(await readFile(new URL("../brand/settings.json", import.meta.url), "utf8"));
  const readme = await readFile(new URL("../README.md", import.meta.url), "utf8");
  assert.ok(readme.includes(`<strong>${brand.words.tagline}</strong>`), "README lead matches brand");
  assert.ok(html.includes(`<h1>${brand.words.tagline}</h1>`), "site lead matches README and brand");
  assert.match(html, /formatted citations in APA, Chicago, Harvard, IEEE and MLA/);
  assert.doesNotMatch(html, /\{\{\w+\}\}/);
  const version = JSON.parse(await readFile(new URL("../.release-please-manifest.json", import.meta.url), "utf8"))["."];
  assert.ok(html.includes(`Early preview · <a href="https://github.com/ibiscite/ibis/releases/tag/v${version}">${version}</a>`), "the sidebar names the released version");
  const design = await readFile(new URL("../docs/design.md", import.meta.url), "utf8");
  const highlights = [...html.matchAll(/<li><strong>([^<]+)<\/strong>/g)];
  assert.equal(highlights.length, 3);
  for (const name of ["Page numbers and locators", "Zotero while closed", "Insertion"]) assert.ok(design.includes(name));
  assert.match(html, /not a stable release/);
  assert.match(html, /does not generate your document’s bibliography/);
  assert.ok(html.indexOf("not a stable release") < html.indexOf("/releases/download/"));
  for (const [, target] of html.matchAll(/href="#([^"\s]+)"/g)) assert.ok(html.includes(`id="${target}"`), target);
});

test("keys lead the example and roadmap highlights omit version numbers", async () => {
  const html = await content(await readFile(new URL("../website/index.html", import.meta.url), "utf8"));
  const how = html.match(/<section id="how"[\s\S]*?<\/section>/)[0];
  assert.ok(how.indexOf("formatted citations") > how.indexOf("data-result"));
  const roadmap = html.match(/<section id="roadmap"[\s\S]*?<\/section>/)[0];
  assert.doesNotMatch(roadmap, /Planned for|\b0\.\d/);
  assert.doesNotMatch(roadmap, /<strong>Insertion<\/strong>|cloud-synced data/);
});

test("macOS browser platform names select Mac shortcuts and native tokens", async () => {
  const { runInNewContext } = await import("node:vm");
  const script = await readFile(new URL("../website/site.js", import.meta.url), "utf8");
  for (const platform of ["macOS", "MacIntel"]) {
    const dataset = {};
    runInNewContext(script, { document: { documentElement: { dataset }, addEventListener() {} },
      navigator: { userAgentData: { platform }, platform: "ignored" }, localStorage: { getItem: () => null } });
    assert.equal(dataset.platform, "macos");
  }
});
