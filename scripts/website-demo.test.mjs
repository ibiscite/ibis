import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { stripTypeScriptTypes } from "node:module";
import { runInNewContext } from "node:vm";
import { test } from "node:test";
import { createDemo, references, formats, noticeCandidates } from "../website/demo-model.mjs";

// Like the page-control tests, run the real script against a small synthetic DOM.
const keyScript = stripTypeScriptTypes(await readFile(new URL("../apps/desktop/src/keys.ts", import.meta.url), "utf8")).replaceAll(/^export /gm, "");
const demoScript = (await readFile(new URL("../website/demo.js", import.meta.url), "utf8")).replaceAll(/^import .*;\n/gm, "").replace("export function setupDemo", "function setupDemo");
function demoPage(platform) {
  const element = tag => ({ tag, children: [], attrs: {}, handlers: {}, dataset: {}, className: "", classList: { toggle() {} },
    append(...children) { this.children.push(...children); },
    replaceChildren(...children) { this.children = children; },
    get textContent() { return this.children.map(child => typeof child === "string" ? child : child.textContent).join(""); },
    set textContent(text) { this.children = [text]; },
    get firstChild() { return this.children[0]; },
    setAttribute(key, value) { this.attrs[key] = String(value); }, removeAttribute(key) { delete this.attrs[key]; }, toggleAttribute() {},
    addEventListener(key, fn) { this.handlers[key] = fn; }, insertAdjacentHTML() {}, focus() {},
    querySelectorAll(selector) { return this.children.filter(child => typeof child !== "string").flatMap(child => [
      ...((selector.startsWith(".") ? child.className.split(" ").includes(selector.slice(1)) : child.tag === selector) ? [child] : []),
      ...child.querySelectorAll(selector),
    ]); },
    querySelector(selector) { return this.querySelectorAll(selector)[0]; },
    replaceWith(replacement) { this.replacement = replacement; }, getBoundingClientRect() { return { bottom: 0 }; },
  });
  const figure = element("figure"), result = element("p"), section = element("section"), list = element("ol"), placeholder = element("div");
  placeholder.className = "picker-panel";
  const receipt = element("div"), output = element("code"), citation = element("span");
  receipt.className = "copied-notice"; citation.className = "notice-citation-full";
  output.append(citation); receipt.append(output); figure.append(placeholder, receipt);
  const sentence = element("span"); sentence.textContent = "Invented sentence"; result.append(sentence);
  figure.closest = selector => selector === "section" ? section : list;
  const document = { ...element("document"), documentElement: { dataset: { platform } }, createElement: element,
    createElementNS: (_namespace, tag) => element(tag), querySelector: selector => selector === "[data-picker]" ? figure : result };
  runInNewContext(`${keyScript}\n${demoScript}\nsetupDemo();`, { document, createDemo, references, formats, noticeCandidates,
    typePaths: { book: [], article: [] }, matchMedia: () => ({ matches: false }), ResizeObserver: class { observe() {} } });
  const panel = placeholder.replacement, [add, copy] = panel.querySelector(".picker-actions").children, input = panel.querySelector("input");
  return { add, copy, input, panel, reopen: figure.querySelector("button") };
}

for (const [platform, addCaps, copyCaps, pluses] of [
  ["macos", ["⇧↵"], ["↵"], []],
  ["windows", ["Shift", "Enter"], ["Enter"], []],
  ["linux", ["Shift", "Enter"], ["Enter"], ["+"]],
]) test(`${platform} footer keeps system key caps through Add, Remove, no-match search and reopen`, () => {
  const { add, copy, input, panel, reopen } = demoPage(platform);
  const check = label => {
    assert.equal(add.attrs["aria-label"], label);
    assert.equal(add.attrs["aria-keyshortcuts"], "Shift+Enter");
    assert.equal(copy.attrs["aria-keyshortcuts"], "Enter");
    for (const [button, caps, separators] of [[add, addCaps, pluses], [copy, copyCaps, []]]) {
      assert.deepEqual(button.querySelectorAll("kbd").map(cap => cap.textContent), caps);
      assert.deepEqual(button.querySelectorAll(".key-plus").map(plus => plus.textContent), separators);
      assert.equal(button.querySelector(".key-caps").attrs["aria-hidden"], "true");
    }
  };
  check("Add"); add.handlers.click(); check("Remove"); add.handlers.click(); check("Add");
  input.value = "no invented reference matches"; input.handlers.input(); check("Add");
  assert.equal(add.disabled, true); assert.equal(copy.disabled, true);
  const escape = { key: "Escape", ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, target: input, preventDefault() {} };
  for (let step = 0; step < 3; step++) panel.handlers.keydown(escape);
  assert.equal(panel.hidden, true);
  reopen.handlers.click(); check("Add"); assert.equal(panel.hidden, false);
});

test("shared cap labels retain native modifier order and literal plus and slash keys", () => {
  const ctx = {}; runInNewContext(keyScript, ctx);
  for (const [accelerator, mac, expected] of [
    ["CmdOrCtrl+Shift+Alt+C", true, ["⌥⇧⌘C"]],
    ["CmdOrCtrl+Shift+Alt+C", false, ["Ctrl", "Shift", "Alt", "C"]],
    ["+", true, ["+"]], ["+", false, ["+"]],
    ["CmdOrCtrl++", true, ["⌘+"]], ["CmdOrCtrl++", false, ["Ctrl", "+"]],
    ["CmdOrCtrl+/", true, ["⌘/"]], ["CmdOrCtrl+/", false, ["Ctrl", "/"]],
  ]) assert.deepEqual(Array.from(ctx.keyCapLabels(accelerator, mac)), expected, `${accelerator}, mac=${mac}`);
});

test("footer key caps use the app's height, radius, fill and label type", async () => {
  const css = await readFile(new URL("../website/demo.css", import.meta.url), "utf8");
  const caps = css.match(/\.picker-actions kbd \{([^}]+)\}/)?.[1] ?? "";
  for (const declaration of ["height: var(--key-cap)", "min-width: var(--key-cap)", "border-radius: var(--r-key)",
    "background: var(--fill-control)", "var(--type-label-size)", "var(--font-family)", "font-weight: 500", "box-shadow: none"]) assert.ok(caps.includes(declaration), declaration);
  assert.match(css, /\.picker-actions \.key-caps \{[^}]*direction: ltr; unicode-bidi: isolate;/, "whole chords stay LTR in RTL pages");
  assert.doesNotMatch(css, /\.picker-foot kbd[^}]*font-size:\s*\.95em/, "footer caps keep label type at all widths");
});

test("notice summaries count omitted references and retain the sample marker", () => {
  const keys = Object.freeze(["vale2024lanterns", "fen2023clouds", "reed2025maps"]);
  assert.deepEqual(noticeCandidates("pandoc", keys), [
    "[@vale2024lanterns; @fen2023clouds; @reed2025maps] [Sample]",
    "[@vale2024lanterns; @fen2023clouds] [Sample] +1 more",
    "[@vale2024lanterns] [Sample] +2 more",
    "+3 more [Sample]",
  ]);
  assert.deepEqual(noticeCandidates("parencite", keys.slice(0, 2)), [
    "\\parencites{vale2024lanterns}{fen2023clouds} [Sample]",
    "\\parencite{vale2024lanterns} [Sample] +1 more",
    "+2 more [Sample]",
  ]);
});

test("the website uses exactly the desktop's bundled invented references", async () => {
  const samples = JSON.parse(await readFile(new URL("../apps/desktop/src-tauri/resources/samples.csljson", import.meta.url), "utf8"));
  assert.equal(samples.length, 3);
  assert.deepEqual(references, samples.map(sample => ({
    key: sample["citation-key"], kind: sample.type === "book" ? "book" : "article",
    authors: sample.author.map(author => author.family).join(", "),
    year: sample.issued["date-parts"][0][0], title: sample.title,
    where: sample["container-title"] || sample.publisher,
  })));
});

test("key demo searches fields, selects and combines known samples in order", () => {
  const demo = createDemo();
  assert.equal(demo.output(), "[@vale2024lanterns] [Sample]");
  demo.setQuery("vale landscapes 2024");
  assert.equal(demo.found().length, 1);
  demo.add("fen2023clouds"); demo.add("vale2024lanterns"); demo.add("unknown");
  for (const [format, expected] of Object.entries({ pandoc: "[@fen2023clouds; @vale2024lanterns]", cite: "\\cite{fen2023clouds,vale2024lanterns}", autocite: "\\autocites{fen2023clouds}{vale2024lanterns}", parencite: "\\parencites{fen2023clouds}{vale2024lanterns}", textcite: "\\textcites{fen2023clouds}{vale2024lanterns}", typst: "@fen2023clouds @vale2024lanterns", keys: "fen2023clouds, vale2024lanterns" })) {
    demo.state.format = format; assert.equal(demo.output(), `${expected} [Sample]`, format);
  }
});

test("empty and no-match searches preserve a prepared citation; no selection refuses copy", () => {
  const demo = createDemo();
  demo.setQuery("");
  assert.equal(demo.found().length, 3);
  demo.add("vale2024lanterns");
  demo.setQuery("no invented reference matches this");
  assert.deepEqual(demo.found(), []);
  assert.equal(demo.output(), "[@vale2024lanterns] [Sample]");
  assert.equal(demo.copy(), true);
  demo.add("vale2024lanterns");
  assert.equal(demo.output(), "");
  assert.equal(demo.copy(), false);
  assert.equal(demo.state.copied, "[@vale2024lanterns] [Sample]");
});

test("copied receipt survives later edits and failed copy; paste uses that receipt", () => {
  const demo = createDemo();
  assert.equal(demo.copy(), true);
  demo.setQuery("clouds"); demo.state.format = "cite";
  demo.paste(); assert.equal(demo.state.pasted, "[@vale2024lanterns] [Sample]");
  assert.equal(demo.copy(), true);
  assert.equal(demo.state.pasted, "[@vale2024lanterns] [Sample]");
  demo.paste(); assert.equal(demo.state.pasted, "\\cite{fen2023clouds} [Sample]");
  demo.setQuery("not present ".repeat(10000));
  assert.equal(demo.copy(), false);
  demo.paste(); assert.equal(demo.state.pasted, "\\cite{fen2023clouds} [Sample]");
});

test("close and reopen preserves choices; reload starts the same practice task afresh", () => {
  const demo = createDemo(); demo.add("vale2024lanterns"); demo.copy();
  demo.back(); demo.back(); demo.back();
  assert.equal(demo.state.open, false);
  demo.state.open = true;
  assert.deepEqual(demo.state.added, ["vale2024lanterns"]);
  assert.equal(demo.state.copied, "[@vale2024lanterns] [Sample]");
  demo.state.format = "unknown";
  assert.equal(demo.copy(), false);
  assert.equal(demo.state.copied, "[@vale2024lanterns] [Sample]");
  const reloaded = createDemo();
  assert.equal(reloaded.state.query, "lantern");
  assert.deepEqual(reloaded.state.added, []);
  assert.equal(reloaded.state.copied, null);
  assert.equal(reloaded.state.pasted, null);
  assert.equal(reloaded.output(), "[@vale2024lanterns] [Sample]");
});
