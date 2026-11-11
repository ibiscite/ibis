import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { stripTypeScriptTypes } from "node:module";
const text = path => readFile(new URL(path, import.meta.url), "utf8");
export const escape = value => value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll('"', "&quot;");
export const pages = [
  { path: "/", file: "home", label: "Home", title: "{{headline}}", description: "A small, free citation picker for researchers and students on Mac, Windows and Linux." },
  { path: "/get/", file: "get", label: "Get", title: "Get Ibis", description: "Download Ibis for Mac, Windows or Linux and choose your reference library." },
  { path: "/today/", file: "today", label: "Today", title: "Today", description: "What Ibis does in the current release, what is in the nightly and what is planned." },
  { path: "/cli/", file: "cli", label: "Command line", title: "Command line and AI agents", description: "Real Ibis command line examples and read-only citation tools for AI agents." },
];
// The sidebar: pages, and the home page's own sections.
export const sections = [
  { label: "Try it", key: "try", href: "/#try" },
  { label: "Where you write", key: "where", href: "/#where" },
  { label: "Get Ibis", key: "get", href: "/get/" },
  { label: "What it does today", key: "today", href: "/today/" },
  { label: "Command line", key: "cli", href: "/cli/" },
  { label: "Why Ibis?", key: "why", href: "/#why" },
];
export const notFound = { path: "/404.html", title: "Page not found", description: "This Ibis page could not be found." };
// Each command and its real output in one terminal block; Copy takes the command alone.
const transcripts = text => text.trim().split(/\n(?=\$ )/).map(block => {
  const [command, ...output] = block.split("\n"), line = command.slice(2);
  return `<div class="code terminal"><pre><code><span class="prompt" aria-hidden="true">$ </span>${escape(line)}${output.map(line => "\n" + (line.startsWith("ibis: ") ? `<span class="stderr">${escape(line)}</span>` : escape(line))).join("")}</code></pre><button class="copy" type="button" data-copy="${escape(line)}" data-copied="Copied" data-failed="Select to copy" title="Copy"><svg class="icon icon-copy" viewBox="0 0 24 24" aria-hidden="true"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg><svg class="icon icon-check" viewBox="0 0 24 24" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg><span>Copy</span></button></div>`;
}).join("\n");
export async function content(html, page = pages[0], input = new URL(".", import.meta.url)) {
  const read = path => readFile(input instanceof URL ? new URL(path, input) : join(input, path), "utf8");
  const headline = JSON.parse(await text("../brand/settings.json")).words.tagline;
  const version = JSON.parse(await text("../.release-please-manifest.json"))["."];
  const keys = await import(`data:text/javascript,${encodeURIComponent(stripTypeScriptTypes(await text("../apps/desktop/src/keys.ts")))}`);
  const status = JSON.parse(await read("content/status.json"));
  const headings = { current: "In the current release", nightly: "In the nightly", planned: "Planned", notYet: "Not yet" };
  const lists = Object.entries(headings).map(([key, title]) => `<section id="${key}" class="row" aria-labelledby="${key}-h"><p class="margin"><span class="key">@${key.toLowerCase()}</span></p><div class="text"><h2 id="${key}-h">${title}</h2><ul class="roadmap" data-status="${key}">${status[key].map(item => `<li><strong>${escape(item.text)}</strong>${item.note ? `<span>${escape(item.note)}</span>` : ""}</li>`).join("")}</ul></div></section>`).join("\n");
  const apps = JSON.parse(await read("content/writing-apps.json"));
  const writing = `<div class="chips" role="tablist" aria-label="Where you write">${apps.map((app, i) => `<button class="key" type="button" role="tab" id="app-${app.id}" aria-controls="writing-example" aria-selected="${i === 0}" tabindex="${i === 0 ? 0 : -1}" title="${escape(app.name)}" data-output="${escape(app.output)}" data-explain="${escape(app.name)}: ${escape(app.note)}">@${app.id}</button>`).join("")}</div><div class="where-doc" id="writing-example" role="tabpanel" aria-labelledby="app-${apps[0].id}"><p class="draft">Lantern gardens follow the rhythm of imaginary cities <code class="key" data-out>${escape(apps[0].output)}</code><span class="caret" aria-hidden="true"></span></p><p class="small" data-explanation>${escape(apps[0].name)}: ${escape(apps[0].note)}</p></div>`;
  const values = { body: "", title: page.title, description: page.description, path: page.path, headline, version,
    robots: page === notFound ? '<meta name="robots" content="noindex">' : "",
    navigation: sections.map(item => { const href = page.path === "/" && item.href.startsWith("/#") ? item.href.slice(1) : item.href; return `<li><a href="${href}"${page.path === item.href ? ' aria-current="page"' : ""}><span class="label">${item.label}</span> <span class="key">@${item.key}</span></a></li>`; }).join(""),
    page: page.file ?? "missing", status: lists, terminal: transcripts(await read("content/terminal.txt")), "writing-apps": writing,
    "shorthand-result": escape(apps[0].output),
    "shortcut-mac": escape(keys.keyLabel(keys.DEFAULT_PICKER_SHORTCUT, true)),
    "shortcut-other": escape(keys.keyLabel(keys.DEFAULT_PICKER_SHORTCUT, false)),
  };
  return html.replaceAll(/\{\{([\w-]+)\}\}/g, (token, key) => values[key] ?? token).replaceAll("{{headline}}", escape(headline));
}
