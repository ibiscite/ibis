import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import { build } from "../website/build.mjs";
import { anchors, brokenLinks } from "./doc-links.mjs";

await build();
const read = path => readFile(new URL(`../website/${path}`, import.meta.url), "utf8");
const page = path => read(`dist/${path}index.html`);
const text = html => {
  let previous;
  do {
    previous = html;
    html = html.replace(/<[^>]*>/g, "");
  } while (html !== previous);
  return html.replaceAll("&quot;", '"').replaceAll("&gt;", ">").replaceAll("&lt;", "<").replaceAll("&amp;", "&");
};

test("four pages share the shell, section navigation and appearance controls", async () => {
  const paths = ["", "get/", "features/", "cli/"];
  let shared;
  for (const path of paths) {
    const html = await page(path);
    const index = html.match(/<header class="index">[\s\S]*?<\/header>/)[0];
    const nav = index.match(/<nav\b[^>]*>([\s\S]*?)<\/nav>/)[1];
    assert.deepEqual([...nav.matchAll(/href="([^"]+)"/g)].map(m => m[1].replace(/^\/#/, "#")),
      ["#try", "#where", "#why", "/get/", "/features/", "/cli/"]);
    assert.deepEqual([...nav.matchAll(/<span class="label">([^<]+)<\/span> <span class="key">([^<]+)<\/span>/g)].map(m => m.slice(1)),
      [["Try it", "@try"], ["Where you write", "@where"], ["Why Ibis?", "@why"],
        ["Get Ibis", "@get"], ["Features", "@features"], ["Command line", "@cli"]]);
    if (path) assert.deepEqual([...nav.matchAll(/<a href="([^"]+)" aria-current="page"/g)].map(m => m[1]), [`/${path}`]);
    assert.ok(index.indexOf('class="appearance"') > index.indexOf("</nav>"));
    const footer = html.match(/<footer\b[\s\S]*?<\/footer>/)[0];
    for (const area of [index, footer]) assert.match(area, /role="switch" aria-label="Dark appearance"/);
    const shell = (index + footer).replaceAll(' aria-current="page"', "").replaceAll('href="/#', 'href="#');
    if (shared) assert.equal(shell, shared); else shared = shell;
    assert.equal((html.match(/<h1[ >]/g) || []).length, 1);
    assert.doesNotMatch(html, /<details|<summary|style=|<style|\{\{\w+\}\}/);
  }
});

test("home offers a download, working picker illustration, four writing apps and a short letter", async () => {
  const html = await page("");
  const brand = JSON.parse(await readFile(new URL("../brand/settings.json", import.meta.url), "utf8"));
  assert.ok(html.includes(`<h1>${brand.words.tagline}</h1>`));
  assert.match(html, /href="\/get\/" data-get[^>]*>Get the early preview<\/a>/);
  assert.match(html, /data-picker/);
  assert.match(html, /Lantern gardens and the rhythm of imaginary cities/);
  assert.match(html, /Copy as Pandoc/);
  assert.match(html, /Sample references · 3/);
  const picker = html.match(/<figure class="picker"[\s\S]*?<\/figure>/)[0];
  assert.deepEqual([...picker.matchAll(/<strong>([^<]+)<\/strong>/g)].map(m => m[1]),
    ["Lantern gardens and the rhythm of imaginary cities", "A field guide to clockwork clouds", "Mapping the quiet islands of an invented sea"]);
  assert.match(picker, /Recently added/);
  assert.match(picker, /Made-up references\. Nothing leaves this page\./);
  assert.match(picker, /More/);
  const apps = JSON.parse(await read("content/writing-apps.json"));
  const tabs = [...html.matchAll(/<button\b[^>]*role="tab"[^>]*>([^<]+)<\/button>/g)];
  assert.deepEqual(tabs.map(m => m[1]), apps.map(app => `@${app.id}`));
  for (const [i, app] of apps.entries()) {
    assert.ok(tabs[i][0].includes(`title="${app.name}"`));
    assert.equal(text(tabs[i][0].match(/data-output="([^"]+)"/)[1]), app.output);
    assert.equal(text(tabs[i][0].match(/data-explain="([^"]+)"/)[1]), `${app.name}: ${app.note}`);
  }
  assert.match(html, /Enter pastes into the app you came from on macOS and Windows/);
  assert.match(html, /On Linux, and wherever pasting isn’t safe, Ibis copies instead and says why/);
  assert.match(html, /id="why"/);
  assert.match(html, /github.com\/diyanko/);
  assert.match(html, /ibiscite\/ibis\/discussions/);
  const strip = html.match(/<p class="key-strip">([\s\S]*?)<\/p>/)[1];
  assert.deepEqual([...strip.matchAll(/<kbd\b[^>]*>([^<]+)<\/kbd>/g)].map(m => m[1]), ["⇧⌘Y", "↵"]);
  assert.match(strip, /<span>type a few words<\/span>/);
  assert.match(strip, /data-shortcut-other="Ctrl\+Shift\+Y"/);
  assert.ok(html.indexOf('class="signature"') < html.indexOf('<ul class="rows">'));
  assert.doesNotMatch(html, /preview-note|fn-mark|data-practice-task/);
  assert.doesNotMatch(html.match(/<section id="try"[\s\S]*?<\/section>/)[0], /A citation key is/);
});

test("each page ends with the next page in the sidebar's reading order", async () => {
  for (const [path, href, label] of [["", "/get/", "Get Ibis"], ["get/", "/features/", "Features"],
    ["features/", "/cli/", "Command line"], ["cli/", "/#try", "Try it"]]) {
    const html = await page(path), main = html.match(/<main\b[^>]*>([\s\S]*?)<\/main>/)[1];
    const next = main.match(/<nav class="row next-page"[^>]*>([\s\S]*?)<\/div><\/nav>\s*$/)[1];
    assert.deepEqual([...next.matchAll(/<a href="([^"]+)"><span class="next-label">Next<\/span><span class="next-name">([^<]+) <span class="key">/g)].map(m => [m[1], text(m[2])]), [[href, label]]);
  }
});

test("Get shows every download, installation notes and shared setup steps without toggles", async () => {
  const html = await page("get/");
  assert.match(html, /Free for Mac, Windows and Linux\./);
  assert.doesNotMatch(html, /data-download-privacy/);
  const platforms = [...html.matchAll(/<div class="platform" data-os="(mac|windows|linux)">[\s\S]*?<\/div>/g)];
  assert.deepEqual(platforms.map(m => m[1]), ["mac", "windows", "linux"]);
  for (const [section] of platforms) {
    assert.equal((section.match(/<ol class="install">([\s\S]*?)<\/ol>/)[1].match(/<li>/g) || []).length, 2);
    assert.doesNotMatch(section, /type="checkbox"|\shidden(?:[\s=>])/);
  }
  assert.deepEqual([...html.matchAll(/releases\/download\/stable\/([^"/]+)/g)].map(m => m[1]),
    ["Ibis-macOS-Apple-silicon.dmg", "Ibis-macOS-Intel.dmg", "Ibis-Windows-setup.exe", "Ibis-Linux.AppImage", "Ibis-Linux.deb"]);
  const setup = html.match(/<ol class="setup">([\s\S]*?)<\/ol>/)[1];
  assert.deepEqual([...setup.matchAll(/<h3>([^<]+)<\/h3>/g)].map(m => m[1]), ["Choose your library", "Press your shortcut"]);
  assert.match(setup, /Zotero/);
  assert.match(setup, /<kbd\b[^>]*>[^<]+<\/kbd> on macOS/);
  assert.match(setup, /<kbd\b[^>]*>[^<]+<\/kbd> on Windows and Linux X11/);
  assert.match(platforms[0][0], /System Settings → Privacy &amp; Security → Open Anyway/);
  assert.match(platforms[1][0], /More info → Run anyway/);
  const windows = platforms[1][0];
  assert.doesNotMatch(windows.match(/<ol class="install">([\s\S]*?)<\/ol>/)[1], /Smart App Control|work computer/);
  assert.match(windows, /<p class="small">[^<]*Smart App Control[^<]*work computer[^<]*<a href="https:\/\/github.com\/ibiscite\/ibis\/blob\/main\/docs\/compatibility.md/);
  assert.ok(html.includes('Files come from GitHub. <a href="https://github.com/ibiscite/ibis/releases/latest">Release notes</a>'));
  assert.equal((html.match(/releases\/tag\/nightly/g) || []).length, 1);
  assert.doesNotMatch(html, /<details\b|<summary\b/);
});

test("Features renders all four status lists from one source and credits shipped 0.3.2 features", async () => {
  const html = await page("features/");
  const status = JSON.parse(await read("content/status.json"));
  const main = html.match(/<main\b[^>]*>([\s\S]*?)<\/main>/)[1];
  assert.deepEqual([...main.matchAll(/<h2[^>]*>([^<]+)<\/h2>/g)].map(m => m[1]),
    ["In the current release", "In the nightly", "Planned", "Not yet"]);
  const lists = [...html.matchAll(/<ul class="roadmap" data-status="([^"]+)">([\s\S]*?)<\/ul>/g)];
  assert.deepEqual(lists.map(m => m[1]), Object.keys(status));
  for (const [, key, list] of lists) assert.deepEqual([...list.matchAll(/<li>([\s\S]*?)<\/li>/g)].map(m => text(m[1])),
    status[key].map(item => item.text + (item.note ?? "")));
  assert.match(html, /Prefix, suffix and Keep My Order/);
  assert.match(html, /0\.3\.2/);
  assert.match(html, /blob\/main\/ROADMAP.md/);
  assert.ok(html.indexOf("Plans can change") < html.indexOf('id="current"'));
});

test("CLI prints the captured transcript verbatim, explains stdout and offers installation", async () => {
  const html = await page("cli/");
  assert.match(html, /<h1>Command line<\/h1>/);
  const terminal = await read("content/terminal.txt");
  const blocks = [...html.matchAll(/<div class="code terminal">[\s\S]*?<pre><code>([\s\S]*?)<\/code><\/pre>[\s\S]*?<\/div>/g)];
  // Each separate terminal block contributes one line break to the transcript.
  const visible = blocks.map(m => text(m[1]) + "\n").join("");
  assert.equal(visible, terminal);
  const commands = [...html.replace(/<section id="agents"[\s\S]*?<\/section>/, "").matchAll(/<div class="code(?: \w+)*">[\s\S]*?<pre><code>([\s\S]*?)<\/code><\/pre>([\s\S]*?)<\/div>/g)];
  assert.equal(commands.length, blocks.length + 1); // The Homebrew installer.
  for (const [, code, controls] of commands) {
    const command = text(code).split("\n")[0].replace(/^\$ /, "");
    const buttons = [...controls.matchAll(/<button\b[^>]*data-copy="([^"]+)"[^>]*>[\s\S]*?<span>([^<]+)<\/span><\/button>/g)];
    assert.deepEqual(buttons.map(m => [text(m[1]), m[2]]), [[command, "Copy"]]);
  }
  for (const text of ["standard output", "standard error", "pbcopy", "clip", "wl-copy", "brew install ibiscite/tap/ibis", "ibis mcp", "docs/cli.md"]) assert.ok(html.includes(text), text);
});

test("assistant chips offer exactly one action, starting with a copyable setup prompt without JavaScript", async () => {
  const html = (await page("cli/")).match(/<section id="agents"[\s\S]*?<\/section>/)[0];
  const apps = ["any", "claude", "codex", "cursor", "vscode", "chatgpt"];
  const tabs = [...html.matchAll(/<button\b[^>]*role="tab"[^>]*>([^<]+)<\/button>/g)];
  assert.deepEqual(tabs.map(m => m[1]), apps.map(app => `@${app}`));
  const panels = [...html.matchAll(/<div\b[^>]*role="tabpanel"[^>]*>[\s\S]*?(?=<\/div>\s*(?:<div\b[^>]*role="tabpanel"|<\/div>))/g)];
  assert.equal(panels.length, apps.length);
  const setup = [
    "Set up Ibis for me: https://ibiscite.org/setup",
    "claude mcp add --scope user ibis -- ibis mcp --app-libraries",
    "codex mcp add ibis -- ibis mcp --app-libraries",
  ];
  const guide = await readFile(new URL("../docs/agents.md", import.meta.url), "utf8");
  for (const path of ["README.md", "docs/cli.md"]) {
    const doc = await readFile(new URL(`../${path}`, import.meta.url), "utf8");
    assert.deepEqual(doc.match(/^Set up Ibis for me:.*$/gm), [setup[0]], `${path} offers the same prompt`);
  }
  assert.deepEqual(brokenLinks(["docs/agents.md"]), []);
  for (const [i, app] of apps.entries()) {
    const tab = tabs[i][0], panel = panels[i][0];
    const id = tab.match(/id="([^"]+)"/)[1], target = tab.match(/aria-controls="([^"]+)"/)[1];
    assert.ok(panel.includes(`id="${target}"`));
    assert.ok(panel.includes(`aria-labelledby="${id}"`));
    assert.ok(tab.includes(`aria-selected="${i === 0}"`));
    assert.ok(panel.includes(`aria-hidden="${i !== 0}"`));
    assert.equal(/\binert\b/.test(panel), i !== 0);
    const examples = [...panel.matchAll(/<div class="code single(?: prose)?"><pre><code>([\s\S]*?)<\/code><\/pre>[\s\S]*?<button\b[^>]*data-copy="([^"]+)"/g)];
    const links = [...panel.matchAll(/<a\b[^>]*href="((?:cursor:|vscode:)[^"]+)"[^>]*>([\s\S]*?)<\/a>/g)];
    const sentences = [...panel.matchAll(/<p>([\s\S]*?)<\/p>/g)];
    assert.equal(examples.length + links.length + sentences.length, 1, `${app} offers exactly one action`);
    if (i < setup.length) {
      assert.equal(text(examples[0][1]), setup[i]);
      assert.equal(text(examples[0][2]), setup[i], `${app} copies the whole line`);
      if (app !== "any") assert.ok(setup[i].includes("--app-libraries"));
    }
    if (app === "cursor" || app === "vscode") {
      const url = new URL(text(links[0][1]));
      const config = app === "cursor" ? JSON.parse(Buffer.from(url.searchParams.get("config"), "base64").toString()) : JSON.parse(decodeURIComponent(url.search.slice(1)));
      assert.equal(config.command, "ibis");
      assert.deepEqual(config.args, ["mcp", "--app-libraries"]);
      assert.equal(text(links[0][2]).trim(), app === "cursor" ? "Add to Cursor" : "Add to VS Code");
    }
    if (app === "chatgpt") assert.equal(text(sentences[0][1]), "ChatGPT desktop: Settings › MCP servers › Add server, command ibis mcp --app-libraries. Not available in ChatGPT on the web.");
    const anchor = tab.match(/data-guide="([^"]*)"/)[1];
    assert.equal(anchor === "", app === "any");
    if (anchor) assert.ok(anchors(guide).has(anchor.slice(1)), `${app} guide anchor resolves offline`);
  }
  for (const copy of ["@agents", "Your library, in your AI assistant", "Find references and format citations from the libraries you choose.", "Try asking", "“Find the lantern gardens paper in my library and cite it as Pandoc.”", "Paste into your assistant", "Read-only access to the libraries you choose; your AI provider’s privacy policy applies."]) assert.ok(text(html).includes(copy), copy);
  assert.match(html, /href="\/cli\/#install"/);
  assert.match(text(html), /Install ibis first/);
  assert.match(html, /href="https:\/\/github.com\/ibiscite\/ibis\/blob\/main\/docs\/agents.md" data-agent-guide>Setup guide/);
  assert.match(html, /class="draft"/);
});
