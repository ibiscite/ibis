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
  const planned = html.match(/<ul class="roadmap" data-planned>[\s\S]*?<\/ul>/)[0];
  assert.equal([...planned.matchAll(/<li><strong>([^<]+)<\/strong>/g)].length, 3);
  for (const name of ["Citation forms", "Browse", "`ibis check`"]) assert.ok(design.includes(name));
  assert.doesNotMatch(planned, /Windows too|reader/i, "shipped paste on Windows is not a plan; the PDF reader is not claimed");
  const nightly = html.match(/<ul class="roadmap" data-nightly>[\s\S]*?<\/ul>/)[0];
  assert.ok(html.indexOf("data-nightly") < html.indexOf("data-planned"), "what has shipped comes before what is planned");
  assert.doesNotMatch(nightly, /PDF|passage|highlight/i, "the reader and passages are planned, not in Nightly");
  assert.match(html, /not a stable release/);
  assert.match(html, /does not generate your document’s bibliography/);
  assert.doesNotMatch(html, /Keep Zotero open|even when Zotero is closed/, "reading a closed Zotero has shipped");
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
  assert.match(html, /<strong>Lantern gardens and the rhythm of imaginary cities<\/strong>/);
  assert.match(html, /Vale · 2024 · Journal of Invented Landscapes/);
  assert.match(html, /<code class="key">\[@vale2024lanterns\]<\/code>/);
  assert.doesNotMatch(html, /Sample ·|\[Sample\]|Sample citations end in/);
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

test("the README and CLI examples use the shared invented references and privacy line", async () => {
  const { references } = await import("../website/demo-model.mjs");
  const read = path => readFile(new URL(path, import.meta.url), "utf8");
  const [readme, cli, help] = await Promise.all([read("../README.md"), read("../docs/cli.md"), read("../crates/ibis-cli/src/lib.rs")]);
  assert.ok(readme.includes("No account, no uploads, no telemetry; sources read without changes."));
  for (const [name, text] of [["README.md", readme], ["docs/cli.md", cli], ["ibis-cli help", help]]) {
    assert.ok(text.includes(references[0].key), `${name} cites the shared first reference`);
    assert.doesNotMatch(text, /garcia 2021|garcia2021|smith2020|jones2019|accents2021|protected2020|search "?memory/, `${name} has no second example set`);
  }
});
