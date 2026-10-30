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
  assert.match(html, /formatted citations in APA, Chicago, Harvard and MLA, and IEEE reference entries/);
  assert.doesNotMatch(html, /\{\{\w+\}\}/);
  assert.ok(html.includes('Early preview · <a href="https://github.com/ibiscite/ibis/releases">Release notes</a>'), "public release link must work while the next version is an unpublished draft");
  const design = await readFile(new URL("../docs/design.md", import.meta.url), "utf8");
  const highlights = [...html.matchAll(/<li><strong>([^<]+)<\/strong>/g)];
  assert.equal(highlights.length, 3);
  for (const name of ["Zotero while closed", "Paste into the app you came from", "Browse"]) assert.ok(design.includes(name));
  assert.match(html, /not a stable release/);
  assert.match(html, /does not generate your document’s bibliography/);
  assert.ok(html.indexOf("not a stable release") < html.indexOf("/releases/download/"));
  for (const [, target] of html.matchAll(/href="#([^"\s]+)"/g)) assert.ok(html.includes(`id="${target}"`), target);
});

test("keys lead the example and roadmap highlights omit version numbers", async () => {
  const html = await content(await readFile(new URL("../website/index.html", import.meta.url), "utf8"));
  const how = html.match(/<section id="how"[\s\S]*?<\/section>/)[0];
  assert.ok(how.indexOf("formatted citations") > how.indexOf("citation-key"));
  const roadmap = html.match(/<section id="roadmap"[\s\S]*?<\/section>/)[0];
  assert.doesNotMatch(roadmap, /Planned for|\b0\.\d/);
  assert.doesNotMatch(roadmap, /<strong>Insertion<\/strong>|<strong>Bibliography export<\/strong>|cloud-synced data/);
});

test("website, first-run help and screenshot instructions teach the same practice task", async () => {
  const html = await content(await readFile(new URL("../website/index.html", import.meta.url), "utf8"));
  const messages = await readFile(new URL("../apps/desktop/src/messages.ts", import.meta.url), "utf8");
  const design = await readFile(new URL("../docs/design.md", import.meta.url), "utf8");
  const help = await readFile(new URL("../website/README.md", import.meta.url), "utf8");
  const task = "Search for “lantern”, choose the lantern gardens reference, then press Enter.";
  assert.equal(messages.match(/sampleSearchHelp: "([^"]+)"/)[1], task);
  assert.deepEqual([...html.matchAll(/data-practice-task>([^<]+)<\/p>/g)].map(match => match[1]), [task]);
  assert.ok(design.includes(task));
  assert.ok(help.includes(task));
  assert.match(html, /Sample · Lantern gardens and the rhythm of imaginary cities/);
  assert.match(html, /Vale · 2024 · Journal of Invented Landscapes/);
  assert.match(html, /\[@vale2024lanterns\] \[Sample\]/);
  assert.doesNotMatch(html, /okafor|Wading birds|wetland change/);
});

test("each download location carries the same privacy sentence and policy link without JavaScript", async () => {
  const html = await content(await readFile(new URL("../website/index.html", import.meta.url), "utf8"));
  const locations = [html.match(/<div class="row intro">[\s\S]*?<\/div><\/div>/)[0],
    ...[...html.matchAll(/<div class="platform"[^>]*>[\s\S]*?<\/div>/g)].map(match => match[0])];
  assert.equal(locations.length, 4);
  const sentence = "No account, no uploads, no telemetry; sources read without changes.";
  const link = '<a href="https://github.com/ibiscite/ibis/blob/main/PRIVACY.md">Privacy</a>';
  for (const location of locations) {
    assert.ok(location.includes(`${sentence} ${link}`), location);
    assert.match(location, /<p class="small" data-download-privacy>/, "ordinary wrapping prose stays beside the download at small widths");
  }
  const privacy = await readFile(new URL("../PRIVACY.md", import.meta.url), "utf8");
  assert.match(privacy, /Ibis reads your libraries without changing them/);
  assert.match(privacy, /Telemetry:\*\* none, ever/);
  assert.match(privacy, /Ibis never sends your libraries, search queries, selected references,/);
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
