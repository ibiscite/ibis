import assert from "node:assert/strict";
import { readFile, mkdir } from "node:fs/promises";
import { runInNewContext } from "node:vm";
import { test } from "node:test";
import { join, resolve } from "node:path";
import { content } from "../website/content.mjs";
import { sharedKeys } from "../website/shared.mjs";
const script = await readFile(new URL("../website/site.js", import.meta.url), "utf8");
const keys = await sharedKeys();
const apps = JSON.parse(await readFile(new URL("../website/content/writing-apps.json", import.meta.url), "utf8"));
const control = () => ({ attrs: {}, handlers: {}, setAttribute(k, v) { this.attrs[k] = v; },
  getAttribute(k) { return this.attrs[k]; }, hasAttribute(k) { return k in this.attrs; },
  removeAttribute(k) { delete this.attrs[k]; }, toggleAttribute(k, value) { if (value) this.attrs[k] = ""; else delete this.attrs[k]; },
  replaceChildren(...children) { this.children = children; },
  addEventListener(k, fn) { this.handlers[k] = fn; }, focus() { this.focused = true; } });
const classes = () => {
  const values = new Set();
  return { contains: value => values.has(value), add: value => values.add(value), remove: value => values.delete(value),
    toggle(value, on) { if (on) values.add(value); else values.delete(value); } };
};
function context(choice = null, platform = "MacIntel", userAgent = "Desktop", maxTouchPoints = 0) {
  const stored = choice instanceof Map ? choice : new Map(choice ? [["ibis-theme", choice]] : []);
  const media = { matches: false, addEventListener(_name, fn) { this.change = fn; } };
  const ctx = { document: { documentElement: { dataset: {} }, addEventListener() {}, querySelector: () => null },
    navigator: { platform, userAgent, maxTouchPoints },
    clearTimeout() {}, getComputedStyle: () => ({ direction: "ltr" }),
    localStorage: { getItem: key => stored.get(key), setItem: (key, value) => stored.set(key, value), removeItem: key => stored.delete(key) }, matchMedia: () => media };
  runInNewContext(script, ctx);
  return { ctx, media, stored };
}

test("both appearance controls stay in sync, persist across pages and reset to system", () => {
  const groups = () => [0, 1].map(() => {
    const toggle = control(), reset = { ...control(), dataset: { system: "System", useSystem: "Use system" } };
    return { hidden: true, toggle, reset, querySelector: selector => selector.includes("switch") ? toggle : reset };
  });
  const mount = ctx => { const controls = groups(); ctx.document.querySelectorAll = () => controls; ctx.appearance(); return controls; };
  const { ctx, stored } = context("light");
  const controls = mount(ctx);
  assert.equal(ctx.document.documentElement.dataset.theme, "light");
  assert.ok(controls.every(group => !group.hidden && group.toggle.attrs["aria-checked"] === "false"));
  controls[1].toggle.handlers.click();
  assert.equal(stored.get("ibis-theme"), "dark");
  assert.ok(controls.every(group => group.toggle.attrs["aria-checked"] === "true" && !group.reset.disabled));
  const next = context(stored), nextControls = mount(next.ctx);
  assert.equal(next.ctx.document.documentElement.dataset.theme, "dark");
  assert.ok(nextControls.every(group => group.toggle.attrs["aria-checked"] === "true"));
  next.media.change();
  assert.ok(nextControls.every(group => group.toggle.attrs["aria-checked"] === "true"));
  nextControls[0].reset.handlers.click();
  assert.equal(stored.has("ibis-theme"), false);
  assert.equal(nextControls[0].toggle.focused, true);
  assert.equal(next.ctx.document.documentElement.dataset.theme, undefined);
  assert.ok(nextControls.every(group => group.toggle.attrs["aria-checked"] === "false" && group.reset.disabled));
  next.media.matches = true; next.media.change();
  assert.ok(nextControls.every(group => group.toggle.attrs["aria-checked"] === "true"));
  assert.equal(context(stored).ctx.document.documentElement.dataset.theme, undefined);
  assert.equal(context("untrusted").ctx.document.documentElement.dataset.theme, undefined);
});

test("detected desktops get a named download; phones keep the generic download", () => {
  for (const [platform, agent, touch, label] of [
    ["MacIntel", "Desktop", 0, "Get Ibis for Mac"], ["macOS", "Desktop", 0, "Get Ibis for Mac"],
    ["Win32", "Desktop", 0, "Get Ibis for Windows"], ["Linux", "Desktop", 0, "Get Ibis for Linux"],
    ["Linux", "Android", 1, "Get the early preview"], ["MacIntel", "Tablet", 5, "Get the early preview"],
    ["iPhone", "iPhone", 1, "Get the early preview"], ["unknown", "Desktop", 0, "Get the early preview"],
  ]) {
    const { ctx } = context(null, platform, agent, touch), button = { textContent: "Get the early preview", href: "/get/", dataset: { mac: "Get Ibis for Mac", windows: "Get Ibis for Windows", linux: "Get Ibis for Linux" } };
    ctx.document.querySelectorAll = () => [];
    ctx.document.querySelector = () => button; ctx.platforms();
    assert.equal(button.textContent, label);
    assert.equal(button.href, "/get/");
  }
});

test("Get keeps every platform visible without adding platform toggles", () => {
  for (const platform of ["MacIntel", "Win32", "Linux"]) {
    const { ctx } = context(null, platform);
    const inserted = [], blocks = ["mac", "windows", "linux"].map(os => ({ hidden: false, dataset: { os }, before: node => inserted.push(node) }));
    ctx.document.querySelectorAll = () => blocks;
    ctx.document.querySelector = () => null;
    ctx.document.createElement = control;
    ctx.platforms();
    assert.deepEqual(blocks.map(block => block.hidden), [false, false, false], platform);
    assert.deepEqual(inserted, []);
  }
});

test("the home key strip uses platform text from its template", async () => {
  const html = await content(await readFile(new URL("../website/pages/home.html", import.meta.url), "utf8"));
  const cap = html.match(/<kbd\b[^>]*data-shortcut-other="([^"]+)"[^>]*>([^<]+)<\/kbd>/);
  for (const [platform, expected] of [["MacIntel", cap[2]], ["Win32", cap[1]], ["Linux", cap[1]]]) {
    const { ctx } = context(null, platform), kbd = { textContent: cap[2], dataset: { shortcutOther: cap[1] } };
    ctx.document.querySelector = () => null; ctx.document.querySelectorAll = () => [kbd]; ctx.platforms();
    assert.equal(kbd.textContent, expected);
  }
});

test("outbound prose and footer links have silent marks; local links, rows, buttons and key chips do not", () => {
  const { ctx } = context();
  const links = [
    ["https://other.test/article", false, true], ["https://other.test/privacy", false, true],
    ["https://ibis.test/get/", false, false], ["https://ibis.test/#try", false, false],
    ["mailto:hello@example.test", false, false],
    ...["rows", "button", "key"].map(() => ["https://other.test/", true, false]),
  ].map(([href, excluded, marked]) => {
    const link = { ...control(), href, marked, closest: () => excluded, append(node) { this.mark = node; } };
    link.text = { textContent: "FUSE support", splitText(index) {
      const word = { textContent: this.textContent.slice(index), replaceWith(node) { link.end = node; } };
      this.textContent = this.textContent.slice(0, index); return word;
    } };
    return link;
  });
  ctx.document.querySelectorAll = () => links;
  ctx.document.createElement = () => ({ ...control(), append(...children) { this.children = children; } });
  ctx.document.createTreeWalker = link => { let pending = true; return { nextNode() { if (!pending) return null; pending = false; return link.text; } }; };
  ctx.location = new URL("https://ibis.test/"); ctx.URL = URL;
  ctx.outboundLinks();
  for (const link of links) {
    assert.equal(!!link.end, link.marked, link.href);
    if (link.end) {
      const [word, mark] = link.end.children;
      assert.equal(link.text.textContent + word.textContent, "FUSE support");
      assert.equal(word.textContent, "support");
      assert.equal(mark.className, "outbound"); assert.equal(mark.attrs["aria-hidden"], "true");
    }
  }
});

test("the picker shortcut opens and focuses home from every inner page, and leaves text entry alone", () => {
  for (const platform of ["MacIntel", "Win32", "Linux"]) for (const path of ["/", "/get/", "/features/", "/cli/"]) {
    const { ctx } = context(null, platform), events = {}, window = control(), destinations = [];
    let opened = 0;
    ctx.window = window; ctx.document.addEventListener = (name, fn) => { events[name] = fn; };
    ctx.document.querySelectorAll = () => [];
    ctx.document.querySelector = () => path === "/" ? {} : null;
    ctx.location = { assign: value => destinations.push(value) };
    ctx.pickerKeys(keys, () => { opened++; });
    const press = extra => ({ key: "Y", code: "KeyY", shiftKey: true, altKey: false,
      metaKey: platform === "MacIntel", ctrlKey: platform !== "MacIntel", target: { closest: () => null },
      preventDefault() { this.prevented = true; }, ...extra });
    for (const extra of [{ target: { closest: () => ({}) } }, { repeat: true }, { isComposing: true }, { shiftKey: false }]) {
      const event = press(extra); events.keydown(event); assert.ok(!event.prevented);
    }
    assert.equal(opened, 0); assert.deepEqual(destinations, []);
    const event = press(); events.keydown(event); assert.equal(event.prevented, true);
    assert.equal(opened, path === "/" ? 1 : 0);
    assert.deepEqual(destinations, path === "/" ? [] : ["/#try"]);
  }
});

test("question mark opens keyboard help outside text entry and ignores held keys or composition", () => {
  const { ctx } = context(), events = {}, button = control(), close = control(), dialog = { ...control(), opened: 0,
    querySelector: () => close, showModal() { this.opened++; }, close() { this.closed = true; } };
  ctx.document.querySelector = selector => selector === ".keyboard-help" ? dialog : button;
  ctx.document.addEventListener = (name, fn) => { events[name] = fn; };
  ctx.keyboardHelp();
  const press = extra => ({ key: "?", target: { closest: () => null }, preventDefault() { this.prevented = true; }, ...extra });
  for (const extra of [{ target: { closest: () => ({}) } }, { repeat: true }, { isComposing: true }, { ctrlKey: true }, { altKey: true }, { metaKey: true }]) {
    const event = press(extra); events.keydown(event); assert.ok(!event.prevented);
  }
  assert.equal(dialog.opened, 0);
  const event = press({ shiftKey: true }); events.keydown(event);
  assert.equal(event.prevented, true); assert.equal(dialog.opened, 1);
  close.handlers.click(); assert.equal(dialog.closed, true);
  button.handlers.click(); assert.equal(dialog.opened, 2);
});

test("shortcut, Enter and Shift+Enter caps stay pressed until their keys are released or the window blurs", () => {
  const { ctx } = context(), events = {}, window = control();
  const caps = ["CmdOrCtrl+Shift+Y", "Enter", "Shift+Enter"].map(accelerator => ({ dataset: { accelerator }, classList: classes() }));
  ctx.window = window; ctx.document.addEventListener = (name, fn) => { events[name] = fn; };
  ctx.document.querySelectorAll = () => caps; ctx.document.querySelector = () => ({});
  ctx.pickerKeys(keys, () => {});
  const event = (key, shiftKey = false, metaKey = false) => ({ key, code: key, shiftKey, metaKey, ctrlKey: false, altKey: false, target: {}, preventDefault() {} });
  const check = expected => assert.deepEqual(caps.map(cap => cap.classList.contains("pressed")), expected);
  events.keydown(event("Enter")); check([false, true, false]);
  events.keydown(event("Shift", true)); check([false, false, true]);
  events.keyup(event("Shift")); check([false, true, false]);
  events.keyup(event("Enter")); check([false, false, false]);
  events.keydown(event("Y", true, true)); check([true, false, false]);
  window.handlers.blur(); check([false, false, false]);
});

test("the bird pecks once per successful action, ignores overlapping actions and respects reduced motion", () => {
  const { ctx, media } = context(), logo = { ...control(), classList: classes(), removeEventListener(name) { delete this.handlers[name]; } };
  let starts = 0;
  const add = logo.classList.add; logo.classList.add = value => { starts++; add(value); };
  ctx.document.querySelector = () => logo;
  ctx.peck(); ctx.peck(); assert.equal(starts, 1);
  logo.handlers.animationend(); assert.equal(logo.classList.contains("pecking"), false);
  ctx.peck(); assert.equal(starts, 2);
  logo.handlers.animationcancel(); media.matches = true; ctx.peck(); assert.equal(starts, 2);
});

function citationChip() {
  const { ctx } = context(), timers = new Map(), events = {}, animations = [];
  const hover = { ...control(), matches: true }, still = { ...control(), matches: false };
  const style = () => ({ removeProperty(name) { if (name === "inline-size") delete this.inlineSize; } });
  const chip = { ...control(), textContent: "@sample", style: style(),
    getBoundingClientRect() { return { width: parseFloat(this.style.inlineSize) || this.textContent.length * 8 + 12 }; },
    cloneNode() { return { ...this, style: style(), remove() {} }; }, after() {},
    getAnimations() { return [{ finished: new Promise(resolve => animations.push(resolve)) }]; } };
  let serial = 0;
  ctx.document.querySelectorAll = () => [chip];
  ctx.document.addEventListener = (name, fn) => { events[name] = fn; };
  ctx.matchMedia = query => query.includes("reduced-motion") ? still : hover;
  ctx.setTimeout = fn => { timers.set(++serial, fn); return serial; }; ctx.clearTimeout = id => timers.delete(id);
  return { ctx, chip, hover, still, events, timers,
    tick() { const [id, fn] = timers.entries().next().value; timers.delete(id); fn(); },
    // Complete newer transitions first, so cancelled work arrives late.
    async settle() {
      await new Promise(setImmediate);
      animations.splice(0).reverse().forEach(resolve => resolve());
      await new Promise(setImmediate);
    } };
}

test("citation pills hug their text at rest, make room before wider forms and return to natural width", async () => {
  const p = citationChip(), natural = p.chip.getBoundingClientRect().width;
  p.ctx.chips();
  assert.equal(p.chip.getBoundingClientRect().width, natural);
  assert.ok(!p.chip.style.inlineSize);
  p.chip.handlers.pointerenter({ pointerType: "mouse" });
  for (const form of ["[@sample]", "\\cite{sample}", "@sample"]) {
    const previous = p.chip.textContent;
    p.tick();
    if (form.length > previous.length) assert.equal(p.chip.textContent, previous, "expansion precedes the wider text");
    await p.settle();
    assert.equal(p.chip.textContent, form);
    assert.equal(p.chip.getBoundingClientRect().width, form.length * 8 + 12);
  }
  assert.ok(!p.chip.style.inlineSize, "the completed cycle restores natural sizing");
  p.tick(); await p.settle();
  p.chip.handlers.pointerleave(); await p.settle();
  assert.equal(p.chip.textContent, "@sample");
  assert.equal(p.chip.getBoundingClientRect().width, natural);
  assert.ok(!p.chip.style.inlineSize);
  assert.equal(p.timers.size, 0);
});

test("leaving or re-entering a citation pill cancels old text; touch, reduced motion and hidden pages stay still", async () => {
  const p = citationChip(); p.ctx.chips();
  p.chip.handlers.pointerenter({ pointerType: "mouse" }); p.tick();
  p.chip.handlers.pointerleave(); await p.settle();
  assert.equal(p.chip.textContent, "@sample", "an old expansion cannot replace the reset text");
  assert.ok(!p.chip.style.inlineSize);
  p.chip.handlers.pointerenter({ pointerType: "mouse" }); p.tick();
  p.chip.handlers.pointerleave(); p.chip.handlers.pointerenter({ pointerType: "mouse" });
  await p.settle(); p.tick(); await p.settle();
  assert.equal(p.chip.textContent, "[@sample]");
  for (const reason of ["motion", "hover", "hidden"]) {
    p.chip.handlers.pointerenter({ pointerType: "mouse" }); p.tick();
    if (reason === "motion") { p.still.matches = true; p.still.handlers.change(); }
    if (reason === "hover") { p.hover.matches = false; p.hover.handlers.change(); }
    if (reason === "hidden") { p.ctx.document.hidden = true; p.events.visibilitychange(); }
    await p.settle();
    assert.equal(p.chip.textContent, "@sample", reason);
    assert.ok(!p.chip.style.inlineSize, reason);
    assert.equal(p.timers.size, 0, reason);
    p.still.matches = false; p.hover.matches = true; p.ctx.document.hidden = false;
  }
  for (const pointerType of ["touch", "mouse"]) {
    p.still.matches = pointerType === "mouse";
    p.chip.handlers.pointerenter({ pointerType });
    assert.equal(p.timers.size, 0);
    assert.ok(!p.chip.style.inlineSize);
  }
});

const shorthandExamples = [["fen ch 2", "[@fen2023clouds, chap. 2]"], ["reed 12-14", "[@reed2025maps, pp. 12–14]"],
  ["see vale 33", "[see @vale2024lanterns, p. 33]"], ["see vale 417", "[see @vale2024lanterns, p. 417]"]];
function shorthandPage() {
  const { ctx, media } = context(), events = {}, timers = new Map();
  let serial = 0;
  const input = { ...control(), value: "see vale 417" }, parts = { ...control(), children: [], dataset: { before: "before", empty: "No match" } }, out = { textContent: "[see @vale2024lanterns, p. 417]", classList: classes() };
  const buttons = shorthandExamples.slice(0, 3).map(([textContent]) => ({ ...control(), textContent }));
  const box = { dataset: { references: JSON.stringify([{ key: "vale2024lanterns", label: "Vale 2024", words: "vale" }, { key: "fen2023clouds", label: "Fen 2023", words: "fen" }, { key: "reed2025maps", label: "Reed and Vale 2025", words: "reed" }]) },
    querySelector: selector => ({ "[data-shorthand-input]": input, "[data-shorthand-parts]": parts, "[data-shorthand-out]": out })[selector], querySelectorAll: () => buttons };
  ctx.document.querySelector = () => box;
  ctx.document.createElement = () => ({ ...control(), dataset: {}, append(...children) { this.children = children; } });
  ctx.document.addEventListener = (name, fn) => { events[name] = fn; };
  ctx.setTimeout = (fn, delay) => { const id = ++serial; timers.set(id, { fn, delay }); return id; };
  ctx.clearTimeout = id => timers.delete(id);
  ctx.getComputedStyle = () => ({ getPropertyValue: () => "120ms" });
  const tick = () => { const [id, timer] = timers.entries().next().value; timers.delete(id); timer.fn(); return timer.delay; };
  return { ctx, media, events, timers, input, parts, out, buttons, tick };
}

test("empty and unmatched shorthand leave an empty insertion point; typing and Try update citations without Enter", () => {
  const p = shorthandPage(); p.ctx.shorthand();
  for (const value of ["", "   ", "missing", "see 417"]) {
    p.input.value = value; p.input.handlers.input();
    assert.equal(p.out.textContent, "");
    assert.equal(p.parts.children.map(item => item.textContent).join(""), value.trim() ? "No match" : "");
  }
  for (const [query, citation] of shorthandExamples) {
    p.input.value = ""; p.input.handlers.input();
    for (const char of query) { p.input.value += char; p.input.handlers.input(); }
    assert.equal(p.out.textContent, citation);
  }
  p.input.value = "fen"; p.input.handlers.input();
  assert.equal(p.out.textContent, "[@fen2023clouds]", "a matching query lands while still being typed");
  p.buttons.forEach((button, index) => { button.handlers.click(); assert.equal(p.out.textContent, shorthandExamples[index][1]); });
});

test("idle shorthand fills the gloss before landing, holds the citation, and stops permanently on focus, input or Try", () => {
  const page = shorthandPage(); page.ctx.shorthand();
  assert.equal(page.timers.values().next().value.delay, 3000);
  for (const [example, citation] of shorthandExamples) {
    page.tick();
    for (let i = 0; i < example.length; i++) {
      page.tick(); assert.equal(page.input.value, example.slice(0, i + 1));
      assert.equal(page.out.textContent, "", "typing has not inserted a citation");
    }
    assert.ok(page.parts.children.length, "the gloss explains the completed query before landing");
    assert.ok(page.tick() >= 300, "a short pause separates typing from landing");
    assert.equal(page.out.textContent, citation);
    assert.equal(page.timers.values().next().value.delay, 2500);
    assert.equal(page.parts.children.at(-1).children[0].textContent, example.startsWith("fen") ? "ch 2" : example.split(" ").at(-1));
  }
  for (const action of ["focus", "input", "try"]) {
    const p = shorthandPage(); p.ctx.shorthand(); p.tick(); p.tick();
    if (action === "try") p.buttons[1].handlers.click(); else p.input.handlers[action]();
    const value = p.input.value; assert.equal(p.timers.size, 0, action);
    p.ctx.document.hidden = true; p.events.visibilitychange(); p.ctx.document.hidden = false; p.events.visibilitychange();
    assert.equal(p.timers.size, 0); assert.equal(p.input.value, value);
  }
});

test("idle shorthand pauses while hidden, waits after interaction and stays still under reduced motion", () => {
  const p = shorthandPage(); p.ctx.shorthand(); p.tick(); p.tick();
  const partial = p.input.value;
  p.ctx.document.hidden = true; p.events.visibilitychange(); assert.equal(p.timers.size, 0);
  assert.equal(p.input.value, partial);
  p.ctx.document.hidden = false; p.events.visibilitychange();
  assert.equal(p.timers.values().next().value.delay, 3000);
  p.events.pointerdown(); assert.equal(p.timers.size, 1); assert.equal(p.timers.values().next().value.delay, 3000);
  p.media.matches = true; p.media.change(); assert.equal(p.timers.size, 0);
  const still = shorthandPage(); still.media.matches = true; still.ctx.shorthand(); assert.equal(still.timers.size, 0);
});

test("writing tabs rewrite one citation, keep their labels and support both arrows, Home and End", () => {
  const { ctx } = context();
  const tabs = apps.map(app => ({ ...control(), id: `app-${app.id}`, textContent: `@${app.id}`,
    dataset: { output: app.output, explain: `${app.name}: ${app.note}` } }));
  const output = { textContent: apps[0].output, classList: { remove() {}, add() {} } }, explanation = {}, panel = control();
  panel.querySelector = selector => ({ "[data-out]": output, "[data-explanation]": explanation })[selector];
  ctx.document.querySelector = () => ({ querySelectorAll: () => tabs, querySelector: () => panel });
  ctx.writingApps();
  const check = i => {
    assert.equal(output.textContent, apps[i].output);
    assert.equal(explanation.textContent, `${apps[i].name}: ${apps[i].note}`);
    assert.deepEqual(tabs.map(tab => tab.textContent), apps.map(app => `@${app.id}`));
    assert.equal(panel.attrs["aria-labelledby"], tabs[i].id);
    assert.deepEqual(tabs.map(tab => tab.attrs["aria-selected"]), apps.map((_, n) => String(n === i)));
    assert.deepEqual(tabs.map(tab => tab.tabIndex), apps.map((_, n) => n === i ? 0 : -1));
  };
  for (let i = 0; i < tabs.length; i++) { tabs[i].handlers.click(); check(i); }
  const press = (i, key, next) => {
    let prevented = false;
    tabs[i].handlers.keydown({ key, preventDefault() { prevented = true; } });
    assert.equal(prevented, true); assert.equal(tabs[next].focused, true); check(next);
  };
  press(3, "ArrowRight", 0); press(0, "ArrowLeft", 3);
  press(3, "Home", 0); press(0, "End", 3);
  press(3, "ArrowLeft", 2); press(2, "ArrowRight", 3);
});

test("RTL writing tabs follow the visual arrow direction", () => {
  const { ctx } = context(), tabs = apps.map(app => ({ ...control(), dataset: { output: app.output, explain: app.note } }));
  const output = { classList: classes() }, explanation = {};
  const panel = { ...control(), querySelector: selector => selector === "[data-out]" ? output : explanation };
  ctx.document.querySelector = () => ({ querySelectorAll: () => tabs, querySelector: () => panel });
  ctx.getComputedStyle = () => ({ direction: "rtl" });
  ctx.writingApps();
  tabs[0].handlers.keydown({ key: "ArrowLeft", preventDefault() {} });
  assert.equal(tabs[1].focused, true);
  assert.equal(output.textContent, apps[1].output);
  tabs[1].handlers.keydown({ key: "ArrowRight", preventDefault() {} });
  assert.equal(tabs[0].focused, true);
});

test("assistant tabs select only the requested setup and keep inactive controls out of keyboard navigation", () => {
  const { ctx } = context();
  const names = ["any", "claude", "codex", "cursor", "vscode", "chatgpt"];
  const tabs = names.map(name => {
    const tab = { ...control(), id: `agent-${name}`, dataset: { guide: name === "any" ? "" : `#${name}` } };
    tab.setAttribute("aria-controls", `agent-${name}-panel`);
    return tab;
  });
  const panels = names.map(name => ({ ...control(), id: `agent-${name}-panel` }));
  const guide = { href: "https://github.com/ibiscite/ibis/blob/main/docs/agents.md" }, base = guide.href;
  ctx.document.querySelector = () => ({ querySelectorAll: selector => selector === '[role="tab"]' ? tabs : panels, querySelector: () => guide });
  ctx.setTimeout = () => assert.fail("setup tabs do not cycle");
  ctx.agentApps();
  const check = selected => {
    assert.deepEqual(tabs.map(tab => tab.attrs["aria-selected"]), names.map((_, i) => String(i === selected)));
    assert.deepEqual(tabs.map(tab => tab.tabIndex), names.map((_, i) => i === selected ? 0 : -1));
    assert.deepEqual(panels.map(panel => panel.attrs["aria-hidden"]), names.map((_, i) => String(i !== selected)));
    assert.deepEqual(panels.map(panel => panel.inert), names.map((_, i) => i !== selected));
    assert.equal(guide.href, base + tabs[selected].dataset.guide, "setup guide follows the selected assistant");
  };
  for (let i = 0; i < tabs.length; i++) { tabs[i].handlers.click(); check(i); }
  for (const [i, key, selected] of [[5, "Home", 0], [0, "End", 5], [5, "ArrowRight", 0], [0, "ArrowLeft", 5]]) {
    tabs[i].handlers.keydown({ key, preventDefault() {} });
    check(selected); assert.equal(tabs[selected].focused, true);
  }
  ctx.getComputedStyle = () => ({ direction: "rtl" });
  tabs[0].handlers.keydown({ key: "ArrowLeft", preventDefault() {} }); check(1);
  tabs[1].handlers.keydown({ key: "ArrowRight", preventDefault() {} }); check(0);
});

test("home navigation keeps one current section at the top, on clicks and through scrolling", () => {
  const { ctx } = context(), bar = { ...control(), scrollLeft: 0, clientWidth: 300, scrollWidth: 300 };
  const sections = ["try", "where", "why"].map((id, i) => ({ id, top: 1000 + i * 1000,
    getBoundingClientRect() { return { top: this.top, bottom: this.top + 900 }; } }));
  const links = sections.map(section => { const link = control(); link.setAttribute("href", `#${section.id}`);
    link.getBoundingClientRect = () => ({ left: 0, right: 100 }); return link; });
  bar.getBoundingClientRect = () => ({ left: 0, right: 300 });
  const events = {}, window = { ...control(), innerHeight: 900, scrollY: 0 };
  ctx.window = window; ctx.requestAnimationFrame = fn => fn();
  ctx.document.documentElement.scrollHeight = 4000;
  ctx.document.addEventListener = (name, fn) => { events[name] = fn; };
  ctx.document.querySelectorAll = () => links;
  ctx.document.querySelector = selector => selector === ".index nav ul" ? bar : sections.find(section => `#${section.id}` === selector);
  const check = i => assert.deepEqual(links.map(link => link.attrs["aria-current"]), sections.map((_, n) => n === i ? "location" : undefined));
  ctx.navigation(); check(0);
  sections[0].top = -800; sections[1].top = 200; window.scrollY = 1800;
  window.handlers.scroll(); check(1);
  sections[2].top = 500;
  events.click({ target: { closest: () => links[2] } }); window.handlers.scroll(); check(2);
  window.handlers.wheel(); window.handlers.scroll(); check(1);
  window.scrollY = 3100; window.handlers.scroll(); check(2);
});

test("an inner page reveals its current key on load without scrolling the document", () => {
  const { ctx } = context(), bar = { ...control(), scrollLeft: 0, clientWidth: 300, scrollWidth: 600,
    getBoundingClientRect: () => ({ left: 0, right: 300 }) };
  const current = { getBoundingClientRect: () => ({ left: 400, right: 480 }) };
  ctx.window = { ...control(), innerHeight: 900, scrollY: 0 };
  ctx.document.documentElement.scrollHeight = 2000;
  ctx.document.querySelectorAll = () => [];
  ctx.document.querySelector = selector => selector === ".index nav ul" ? bar : current;
  ctx.navigation();
  assert.equal(bar.scrollLeft, 196);
  assert.equal(ctx.window.scrollY, 0);
});

test("RTL navigation removes its end fade when the last key is reached", () => {
  const { ctx } = context(), bar = { ...control(), scrollLeft: 0, clientWidth: 300, scrollWidth: 600 };
  ctx.document.querySelectorAll = () => [];
  ctx.document.querySelector = selector => selector === ".index nav ul" ? bar : null;
  ctx.window = { ...control(), innerHeight: 900, scrollY: 0 };
  ctx.navigation();
  assert.equal(bar.hasAttribute("data-at-start"), true);
  assert.equal(bar.hasAttribute("data-at-end"), false);
  bar.scrollLeft = -300; bar.handlers.scroll();
  assert.equal(bar.hasAttribute("data-at-start"), false);
  assert.equal(bar.hasAttribute("data-at-end"), true);
});

test("Copy copies only the command and reports clipboard refusal", async () => {
  for (const allowed of [true, false]) {
    const label = { textContent: "Copy" };
    const { ctx } = context(), button = { ...control(), querySelector: () => label, dataset: { copy: 'ibis cite vale2024lanterns --locator "p. 12" --file sample.bib', copied: "Copied", failed: "Select to copy" } };
    const written = [], timers = [];
    let pecks = 0; ctx.peck = () => { pecks++; };
    ctx.navigator.clipboard = { async writeText(value) { if (!allowed) throw Error("blocked"); written.push(value); } };
    ctx.document.querySelectorAll = () => [button];
    ctx.setTimeout = fn => { timers.push(fn); };
    ctx.copyButtons(); await button.handlers.click();
    assert.deepEqual(written, allowed ? [button.dataset.copy] : []);
    assert.equal(label.textContent, allowed ? "Copied" : "Select to copy");
    assert.equal(pecks, allowed ? 1 : 0);
    timers[0](); assert.equal(label.textContent, "Copy");
  }
  const { ctx } = context(), button = control();
  ctx.document.querySelectorAll = () => [button]; ctx.copyButtons();
  assert.equal(button.hidden, true);
});

test("repeated Copy clicks return to Copy and ignore an older clipboard completion", async () => {
  const { ctx } = context(), label = { textContent: "Copy" }, pending = [], timers = new Map();
  const button = { ...control(), querySelector: () => label, dataset: { copy: "ibis mcp --app-libraries", copied: "Copied", failed: "Select to copy" } };
  let serial = 0;
  ctx.navigator.clipboard = { writeText: () => new Promise((resolve, reject) => pending.push({ resolve, reject })) };
  ctx.document.querySelectorAll = () => [button]; ctx.peck = () => {};
  ctx.setTimeout = fn => { timers.set(++serial, fn); return serial; }; ctx.clearTimeout = id => timers.delete(id);
  ctx.copyButtons();
  const first = button.handlers.click(); pending[0].resolve(); await first;
  const second = button.handlers.click(); pending[1].resolve(); await second;
  for (const fn of [...timers.values()]) fn(); timers.clear();
  assert.equal(label.textContent, "Copy");
  const old = button.handlers.click(), latest = button.handlers.click();
  pending[3].reject(Error("blocked")); await latest;
  pending[2].resolve(); await old;
  assert.equal(label.textContent, "Select to copy");
  assert.equal(button.dataset.state, undefined);
  for (const fn of timers.values()) fn();
  assert.equal(label.textContent, "Copy");
});

// Optional existing browser runtime. All requests are fulfilled from disk; no listener or network.
const hasFocusRing = node => { const css = getComputedStyle(node); return css.outlineStyle !== "none" && parseFloat(css.outlineWidth) > 0; };
async function checkCitationPills(page) {
  await page.mouse.move(0, 0);
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await page.evaluate(() => {
    const pills = [...document.querySelectorAll("main .key:not(code), .index nav .key")].filter(node => node.getClientRects().length);
    const geometry = node => {
      const range = document.createRange(); range.selectNodeContents(node);
      const text = range.getBoundingClientRect(), box = node.getBoundingClientRect(), css = getComputedStyle(node);
      return { text, box, natural: text.width + parseFloat(css.paddingLeft) + parseFloat(css.paddingRight) + parseFloat(css.borderLeftWidth) + parseFloat(css.borderRightWidth) };
    };
    const records = pills.map(node => ({ node, original: node.textContent, seen: new Set(), widths: new Set() }));
    const errors = [];
    function sample() {
      for (const record of records) {
        const { node, original } = record, { text, box } = geometry(node);
        record.seen.add(node.textContent); record.widths.add(box.width);
        if (text.left < box.left - .5 || text.right > box.right + .5) errors.push(`${original}: ${node.textContent} overflows`);
        if (Math.abs((text.left + text.right - box.left - box.right) / 2) > .5) errors.push(`${original}: text is not centred`);
        const label = node.closest(".index nav a")?.querySelector(".label");
        if (label && getComputedStyle(label).position !== "absolute") {
          const link = node.parentElement.getBoundingClientRect();
          if (Math.abs((getComputedStyle(node).direction === "rtl" ? box.left - link.left : box.right - link.right)) > .5) errors.push(`${original}: sidebar pill moved from its aligned edge`);
          if (Math.abs(label.getBoundingClientRect().top - box.top) > box.height) errors.push(`${original}: sidebar label wrapped`);
        }
      }
      if (window.citationCheck.running) requestAnimationFrame(sample);
    }
    window.citationCheck = { running: true, errors, records };
    for (const { node, original } of records) {
      const { box, natural } = geometry(node);
      if (Math.abs(box.width - natural) > .5) errors.push(`${original}: resting width does not hug its text`);
      node.dispatchEvent(new PointerEvent("pointerenter", { pointerType: "mouse" }));
    }
    sample();
  });
  await page.waitForFunction(() => window.citationCheck.records.every(({ node, original, seen }) =>
    seen.has(`\\cite{${original.slice(1)}}`) && node.textContent === original && !node.style.inlineSize));
  const result = await page.evaluate(() => {
    const check = window.citationCheck; check.running = false;
    for (const { node } of check.records) node.dispatchEvent(new PointerEvent("pointerleave"));
    return { errors: [...new Set(check.errors)], records: check.records.map(({ original, seen, widths }) => ({ original, seen: [...seen], widths: widths.size })) };
  });
  assert.deepEqual(result.errors, [], "every cycling pill stays centred and contains its text, including the signature");
  for (const { original, seen, widths } of result.records) {
    assert.deepEqual(seen, [original, `[@${original.slice(1)}]`, `\\cite{${original.slice(1)}}`]);
    assert.ok(widths > 3, `${original}: width passes through intermediate sizes`);
  }
  const pager = page.locator(".next-page .key"), original = await pager.innerText();
  const before = await pager.boundingBox();
  await pager.hover();
  await page.waitForFunction(original => document.querySelector(".next-page .key").textContent !== original, original);
  assert.equal(await pager.evaluate(node => getComputedStyle(node).boxShadow), "none", "hover fills the pill without a ring");
  assert.equal(await page.locator(".next-page a").evaluate(hasFocusRing), false);
  await page.mouse.move(0, 0);
  await page.waitForFunction(original => {
    const node = document.querySelector(".next-page .key"); return node.textContent === original && !node.style.inlineSize;
  }, original);
  assert.equal((await pager.boundingBox()).width, before.width, "pointer leave restores the resting width");
  const link = page.locator(".next-page a");
  await link.evaluate(node => node.addEventListener("click", event => event.preventDefault(), { once: true }));
  await link.click(); assert.equal(await link.evaluate(hasFocusRing), false, "pointer activation has no focus ring");
  await link.press("Tab"); await page.keyboard.press("Shift+Tab");
  assert.equal(await link.evaluate(hasFocusRing), true, "keyboard focus keeps its ring");
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.mouse.move(0, 0);
  await pager.hover();
  await page.waitForTimeout(300);
  assert.equal(await pager.innerText(), original, "reduced motion never cycles");
  assert.equal(await pager.evaluate(node => node.getAnimations().length), 0, "reduced motion never transitions");
  await page.mouse.move(0, 0);
  await page.locator("h1").click();
}
const searchGeometry = node => {
  const css = getComputedStyle(node), box = node.getBoundingClientRect(), rtl = css.direction === "rtl";
  const start = rect => rtl ? box.right - rect.right : rect.left - box.left;
  const input = node.querySelector("input"), query = getComputedStyle(input), mark = [...node.querySelectorAll(".picker-mark img")].find(img => img.getClientRects().length).getBoundingClientRect();
  return { height: box.height, padding: css.padding, mark: [start(mark), mark.top - box.top, mark.width, mark.height],
    query: [start(input.getBoundingClientRect()), query.height, query.padding, query.font, query.letterSpacing], placeholder: getComputedStyle(input, "::placeholder").color };
};
async function serveWebsite(page) {
  const root = resolve("website/dist");
  await page.route("**/*", async route => {
    const url = new URL(route.request().url());
    if (url.hostname !== "ibis.test") return route.abort();
    const path = url.pathname.endsWith("/") ? `${url.pathname}index.html` : url.pathname;
    const contentType = path.endsWith(".html") ? "text/html" : path.endsWith(".css") ? "text/css" : /\.(m?js)$/.test(path) ? "text/javascript" : path.endsWith(".svg") ? "image/svg+xml" : undefined;
    await route.fulfill({ body: await readFile(join(root, path)), contentType });
  });
}
test("drafts keep their size through auto-play, manual queries and app changes; the writer's caret stays outside the citation", { skip: !process.env.IBIS_PLAYWRIGHT_MODULE }, async () => {
  const { chromium } = await import(process.env.IBIS_PLAYWRIGHT_MODULE);
  const browser = await chromium.launch({ executablePath: process.env.IBIS_CHROMIUM_EXECUTABLE });
  const caretFollows = node => {
    const caret = node.parentElement.querySelector(".caret");
    if (!caret || node.contains(caret) || !(node.compareDocumentPosition(caret) & Node.DOCUMENT_POSITION_FOLLOWING)) return false;
    if (getComputedStyle(caret).backgroundColor !== getComputedStyle(node.closest(".draft")).color) return false;
    if (!node.textContent) return true;
    const range = document.createRange(); range.selectNodeContents(node);
    range.setStart(node.firstChild, node.firstChild.length - 1);
    const tail = range.getBoundingClientRect(), cursor = caret.getBoundingClientRect();
    return Math.abs(cursor.left - tail.right) < 1.5 && cursor.top < tail.bottom && cursor.bottom > tail.top;
  };
  try {
    for (const width of [320, 375, 640, 992, 1100, 1270, 1600]) {
      const page = await browser.newPage({ viewport: { width, height: 900 }, reducedMotion: "no-preference" });
      await serveWebsite(page);
      await page.clock.install({ time: new Date("2026-11-11T12:00:00Z") });
      await page.clock.pauseAt(new Date("2026-11-11T12:00:01Z"));
      await page.addInitScript(() => { Math.random = () => .5; });
      await page.goto("https://ibis.test/cli/");
      const section = page.locator("#shorthand"), field = page.locator("[data-shorthand-input]"), output = page.locator("[data-shorthand-out]");
      const height = (await section.boundingBox()).height;
      const stable = async () => {
        assert.equal((await section.boundingBox()).height, height, `shorthand keeps its height at ${width}px`);
        assert.equal(await output.evaluate(caretFollows), true);
        assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true, `content fits ${width}px`);
      };
      await stable();
      await page.clock.runFor(3000);
      for (const [query, citation] of shorthandExamples) {
        assert.equal(await field.inputValue(), ""); await stable();
        for (let i = 0; i < query.length; i++) {
          await page.clock.runFor(120);
          assert.equal(await field.inputValue(), query.slice(0, i + 1));
          assert.equal(await output.textContent(), ""); await stable();
        }
        assert.ok(await page.locator(".gloss li").count());
        await page.clock.runFor(600);
        assert.equal(await output.textContent(), citation); await stable();
        await page.clock.runFor(600); await stable();
        assert.equal(await output.evaluate(node => getComputedStyle(node).backgroundColor), "rgba(0, 0, 0, 0)", "the insertion tint ends");
        await page.clock.runFor(1900); await stable();
      }
      for (const query of [...shorthandExamples.map(([query]) => query), "missing"]) {
        await field.fill(""); await stable();
        for (const char of query) { await field.pressSequentially(char); await stable(); }
        if (query === "missing") {
          assert.equal(await output.textContent(), "");
          assert.equal(await page.locator(".gloss").innerText(), "No match in the sample references. Try vale, fen or reed.");
        } else assert.equal(await output.textContent(), shorthandExamples.find(([value]) => value === query)[1]);
      }
      assert.deepEqual(await page.locator("[data-try]").allTextContents(), shorthandExamples.slice(0, 3).map(([query]) => query));
      await page.emulateMedia({ reducedMotion: "reduce" });
      await field.fill("see vale 417");
      const still = async selector => assert.equal(await page.locator(selector).evaluateAll(nodes => nodes.every(node => !node.getAnimations().some(animation => animation.playState === "running"))), true, "Reduce Motion stops blink and tint");
      await still("#shorthand .caret, [data-shorthand-out]");
      assert.equal(await output.evaluate(node => getComputedStyle(node).backgroundColor), "rgba(0, 0, 0, 0)");
      await page.goto("https://ibis.test/");
      const draft = page.locator(".where-doc"), homeOutput = page.locator("[data-out]"), homeHeight = (await draft.boundingBox()).height;
      for (const app of apps) {
        await page.getByRole("tab", { name: `@${app.id}`, exact: true }).click();
        assert.equal((await draft.boundingBox()).height, homeHeight, `Where you write keeps its height at ${width}px`);
        assert.equal(await homeOutput.evaluate(caretFollows), true);
        if (width >= 1100) assert.equal(await homeOutput.evaluate(node => {
          const range = document.createRange(); range.selectNodeContents(node);
          return range.getClientRects().length;
        }), 1, "the longest app citation fits one line at desktop width");
      }
      await still(".where-doc .caret, [data-out]");
      await page.close();
    }
  } finally { await browser.close(); }
});
test("all routes fit the requested viewports in both themes and directions; visitor controls work", { skip: !process.env.IBIS_PLAYWRIGHT_MODULE }, async () => {
  const { chromium } = await import(process.env.IBIS_PLAYWRIGHT_MODULE);
  const browser = await chromium.launch({ executablePath: process.env.IBIS_CHROMIUM_EXECUTABLE, ignoreDefaultArgs: ["--hide-scrollbars"] });
  const root = resolve("website/dist"), screenshots = resolve(".local/website-review");
  const headers = await readFile(join(root, "_headers"), "utf8");
  const csp = headers.match(/^  Content-Security-Policy: (.+)$/m)[1];
  await mkdir(screenshots, { recursive: true });
  try {
    for (const [width, height] of [[1100, 600], [1270, 800], [375, 812]]) for (const colorScheme of ["light", "dark"]) for (const direction of ["ltr", "rtl"]) {
      const page = await browser.newPage({ viewport: { width, height }, colorScheme, reducedMotion: "reduce", hasTouch: width === 375, isMobile: width === 375 });
      const errors = []; page.on("pageerror", error => errors.push(error.message));
      page.on("console", message => { if (message.type() === "error" && /Content Security Policy/i.test(message.text())) errors.push(message.text()); });
      await page.addInitScript(direction => {
        document.addEventListener("DOMContentLoaded", () => { document.documentElement.dir = direction; });
        Object.defineProperty(navigator, "clipboard", { value: { async writeText(value) { window.copiedCommand = value; } } });
      }, direction);
      await page.route("**/*", async route => {
        const url = new URL(route.request().url());
        if (url.hostname !== "ibis.test") return route.abort();
        const path = url.pathname.endsWith("/") ? `${url.pathname}index.html` : url.pathname;
        const type = path.endsWith(".html") ? "text/html" : path.endsWith(".css") ? "text/css" : /\.(m?js)$/.test(path) ? "text/javascript" : path.endsWith(".svg") ? "image/svg+xml" : undefined;
        let body;
        try { body = await readFile(join(root, path)); }
        catch (error) {
          if (error.code !== "ENOENT") throw error;
          return route.fulfill({ path: join(root, "404.html"), status: 404, contentType: "text/html", headers: { "content-security-policy": csp } });
        }
        await route.fulfill({ body, contentType: type, headers: path.endsWith(".html") ? { "content-security-policy": csp } : {} });
      });
      let chosenDark, homeSearch;
      for (const path of ["/", "/get/", "/features/", "/cli/", "/missing/nested/page"]) {
        await page.goto(`https://ibis.test${path}`);
        if (path === "/") {
          await page.getByRole("combobox", { name: "Search the made-up library" }).waitFor();
          homeSearch = await page.locator("[data-picker] .picker-search").evaluate(searchGeometry);
        }
        if (width === 1270 && !path.startsWith("/missing/")) await checkCitationPills(page);
        assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true, `${path} fits ${width} ${colorScheme}`);
        assert.equal(await page.getByRole("heading", { level: 1 }).count(), 1);
        assert.deepEqual(await page.locator("body *").evaluateAll(nodes => nodes.filter(node => {
          if (!node.getClientRects().length) return false;
          const css = getComputedStyle(node);
          return (/auto|scroll/.test(css.overflowY) && node.scrollHeight > node.clientHeight + 1) ||
            (node.tagName !== "PRE" && /auto|scroll/.test(css.overflowX) && node.scrollWidth > node.clientWidth + 1);
        }).map(node => `${node.tagName}.${node.className}`)), [], `${path}: no unexpected nested scrollers at ${width}×${height} ${direction}`);
        const missing = path.startsWith("/missing/");
        assert.equal(await page.locator('.index nav a[aria-current]').count(), missing ? 0 : 1);
        if (!missing) assert.equal(await page.locator('.index nav a[aria-current]').evaluate(node => {
          const chip = node.getBoundingClientRect(), bar = node.closest("ul").getBoundingClientRect();
          return chip.left >= bar.left - 1 && chip.right <= bar.right + 1;
        }), true, `${path} current key is in view`);
        if (width === 375) {
          assert.equal(await page.locator(".index nav .key").count(), 6);
          assert.equal(await page.locator(".index nav ul").evaluate(node => node.scrollWidth <= node.clientWidth + 1), true, "six keys fit without scrolling");
        }
        assert.equal(await page.getByRole("link", { name: "Source on GitHub", exact: true }).count(), 1);
        assert.equal(await page.locator('footer .outbound').count(), 3);
        assert.equal(await page.locator('.rows .outbound, .button .outbound, .key .outbound, a[href^="/#"] .outbound, a[href^="#"] .outbound').count(), 0);
        assert.equal(await page.locator('.outbound').evaluateAll(marks => marks.every(mark => {
          const end = mark.parentElement, range = document.createRange(); range.selectNodeContents(end.firstChild);
          const word = [...range.getClientRects()].at(-1), arrow = mark.getBoundingClientRect();
          return end.classList.contains("outbound-end") && arrow.top < word.bottom && arrow.bottom > word.top;
        })), true, "outbound marks stay with their last word");
        await page.screenshot({ path: join(screenshots, `${path.replaceAll("/", "") || "home"}-${width}-${colorScheme}-${direction}.png`), fullPage: true });
        await page.locator("h1").click();
        await page.keyboard.press("?");
        assert.equal(await page.getByRole("dialog", { name: "Keyboard shortcuts" }).isVisible(), true);
        assert.equal(await page.getByRole("dialog").evaluate(node => node.scrollHeight <= node.clientHeight + 1), true, "keyboard help fits the viewport");
        await page.keyboard.press("Escape");
        assert.equal(await page.getByRole("dialog").isVisible(), false);
        const helpButton = page.getByRole("button", { name: "Keyboard shortcuts ?", exact: true });
        await helpButton.click(); await page.keyboard.press("Escape");
        assert.equal(await helpButton.evaluate(node => node === document.activeElement), true, "Escape returns focus to the help button");
        if (missing) {
          assert.equal(await page.getByRole("heading", { name: "Page not found" }).isVisible(), true);
          if (width >= 992) assert.equal(await page.locator("h1").evaluate(node => {
            const content = node.getBoundingClientRect(), sidebar = document.querySelector(".index").getBoundingClientRect();
            return document.documentElement.dir === "rtl" ? content.right < sidebar.left : content.left > sidebar.right;
          }), true, "404 content clears the sidebar");
          await page.getByRole("link", { name: "Return home", exact: true }).click();
          assert.equal(new URL(page.url()).pathname, "/");
          continue;
        }
        if (chosenDark !== undefined) assert.equal(await page.getByRole("switch").getAttribute("aria-checked"), chosenDark);
        if (path === "/get/") {
          assert.equal(await page.locator('.platform a[href*="/releases/download/"]:visible').count(), 5);
          assert.equal(await page.getByRole("heading", { name: "Choose your library", exact: true }).isVisible(), true);
          assert.equal(await page.getByRole("heading", { name: "Press your shortcut", exact: true }).isVisible(), true);
          assert.equal(await page.locator("details").count(), 0);
        }
        if (path === "/cli/") {
          assert.deepEqual(await page.locator(".shorthand-picker .picker-search").evaluate(searchGeometry), homeSearch, "shorthand uses the demo search geometry, type and placeholder colour");
          const field = page.locator("[data-shorthand-input]"), output = page.locator("[data-shorthand-out]"), firstTry = page.locator("[data-try]").first();
          await field.click();
          assert.equal(await field.evaluate(hasFocusRing), false, "pointer focus gives the search no ring");
          assert.equal(await page.locator(".shorthand-picker .picker-panel").evaluate(hasFocusRing), false, "the panel has no focus outline");
          await field.press("Tab"); await page.keyboard.press("Shift+Tab");
          assert.equal(await field.evaluate(hasFocusRing), true, "keyboard focus rings the search field");
          for (const value of ["", "missing"]) {
            await field.fill(value);
            assert.equal(await field.getAttribute("placeholder"), "Try see vale 417");
            assert.equal(await output.textContent(), "");
            if (value) assert.equal(await page.locator("[data-shorthand-parts]").evaluate(node => {
              const range = document.createRange(); range.selectNodeContents(node);
              const text = range.getBoundingClientRect(), input = document.querySelector("[data-shorthand-input]").getBoundingClientRect();
              return Math.abs(getComputedStyle(node).direction === "rtl" ? text.right - input.right : text.left - input.left) < 1;
            }), true, "no-match text starts at the query's start edge");
            else assert.equal(await page.locator("[data-shorthand-parts]").innerText(), "");
          }
          await firstTry.click();
          assert.equal(await field.inputValue(), "fen ch 2");
          assert.equal(await output.innerText(), "[@fen2023clouds, chap. 2]");
          assert.equal(await firstTry.evaluate(hasFocusRing), false, "Try pointer activation leaves no ring");
          assert.equal(await firstTry.evaluate(node => {
            const row = node.parentElement, gap = row.getBoundingClientRect().top - row.previousElementSibling.getBoundingClientRect().bottom;
            return gap > 0 && Math.abs(gap - parseFloat(getComputedStyle(row).marginBlockStart)) < 1;
          }), true, "Try follows the sentence with paragraph spacing");
          await firstTry.press("Tab"); await page.keyboard.press("Shift+Tab");
          assert.equal(await firstTry.evaluate(hasFocusRing), true, "Try keyboard focus keeps its ring");
        }
        if (path === "/cli/") {
          const region = page.locator("[data-agent-tabs] .tab-panels"), panels = region.locator('[role="tabpanel"]');
          const initialHeight = (await region.boundingBox()).height;
          const installTop = await page.locator("#install").evaluate(node => node.getBoundingClientRect().top + scrollY);
          for (const tab of await page.locator('[data-agent-tabs] [role="tab"]').all()) {
            await tab.click();
            assert.equal((await region.boundingBox()).height, initialHeight, "assistant setup keeps its height across tabs");
            assert.equal(await page.locator("#install").evaluate(node => node.getBoundingClientRect().top + scrollY), installTop, "the next section stays in place");
            const active = region.locator('[role="tabpanel"]:visible');
            assert.equal(await active.count(), 1);
            assert.equal(await active.locator(".code, a, p:not([class])").count(), 1, "each chip shows exactly one setup action");
            const guide = page.locator("[data-agent-guide]");
            assert.equal(new URL(await guide.getAttribute("href")).hash, await tab.getAttribute("data-guide"));
            assert.equal(await panels.evaluateAll(nodes => nodes.every(node => node.inert === (node.getAttribute("aria-hidden") === "true"))), true);
            for (const button of await region.locator(".copy:visible").all()) {
              const command = await button.getAttribute("data-copy");
              await button.click();
              assert.equal(await page.evaluate(() => window.copiedCommand), command);
              if (await tab.innerText() === "@any") assert.equal(command, "Set up Ibis for me: https://ibiscite.org/setup");
              else assert.ok(command.includes("--app-libraries"));
            }
          }
        }
        if (path === "/cli/") for (const button of await page.locator(".copy:visible").all()) {
          const command = await button.getAttribute("data-copy");
          await button.click();
          assert.equal(await page.evaluate(() => window.copiedCommand), command);
          assert.equal(await button.innerText(), "Copied");
          assert.equal(await button.evaluate(hasFocusRing), false, "Copy pointer activation leaves no ring");
        }
        if (path !== "/") {
          const shortcut = await page.evaluate(() => document.documentElement.dataset.platform === "macos" ? "Meta+Shift+Y" : "Control+Shift+Y");
          if (path === "/cli/") {
            await page.locator("[data-shorthand-input]").focus();
            await page.keyboard.press(shortcut);
            assert.equal(new URL(page.url()).pathname, "/cli/");
            await page.locator("h1").click();
          }
          await page.keyboard.press(shortcut);
          await page.waitForURL("https://ibis.test/#try");
          await page.getByRole("combobox", { name: "Search the made-up library" }).waitFor();
          assert.equal(await page.getByRole("combobox", { name: "Search the made-up library" }).evaluate(node => node === document.activeElement), true);
          continue;
        }
        assert.equal(await page.locator(".picker-primary").isVisible(), true);
        assert.equal(await page.locator(".picker-library").isVisible(), true);
        const query = page.getByRole("combobox", { name: "Search the made-up library" });
        assert.equal(await query.inputValue(), "");
        assert.equal(await page.getByRole("listbox").getByRole("option").count(), 3);
        assert.equal(await page.getByRole("listbox", { name: "Recently added" }).isVisible(), true);
        const panelHeight = await page.locator("[data-picker] .picker-panel").evaluate(node => node.getBoundingClientRect().height);
        assert.equal(await page.locator("[data-picker] .picker-search").evaluate(node => {
          const field = node.querySelector(".picker-field").getBoundingClientRect(), controls = node.querySelector(".picker-controls").getBoundingClientRect();
          return Math.abs(field.top + field.height / 2 - controls.top - controls.height / 2) < 1;
        }), true, "search and format share one row");
        assert.equal(await page.locator(".picker-foot").evaluate(node => {
          const foot = node.getBoundingClientRect();
          return [...node.querySelectorAll(".picker-actions > *")].every(control => {
            const rect = control.getBoundingClientRect();
            return rect.top >= foot.top && rect.bottom <= foot.bottom && rect.left >= foot.left && rect.right <= foot.right;
          });
        }), true, "all footer columns fit one row");
        assert.equal(await page.locator(".picker-more").count(), 1);
        const format = page.getByRole("combobox", { name: "Copy format", exact: true });
        if (width === 375) await format.tap(); else await format.click();
        assert.equal(await format.evaluate(node => node.matches(":open")), true, "mouse or touch opens the native format menu");
        await page.keyboard.press("End"); await page.keyboard.press("Enter");
        assert.equal(await format.inputValue(), "keys");
        assert.equal(await page.locator(".picker-shelf code").innerText(), "vale2024lanterns");
        assert.equal(await page.getByRole("button", { name: "Copy as Keys only", exact: true }).isVisible(), true);
        await format.focus(); await page.keyboard.press("Space");
        assert.equal(await format.evaluate(node => node.matches(":open")), true, "keyboard opens the native menu");
        await page.keyboard.press("Home"); await page.keyboard.press("Enter");
        assert.equal(await format.inputValue(), "pandoc");
        await query.focus();
        assert.equal(await query.evaluate(node => getComputedStyle(node).outlineStyle !== "none" && parseFloat(getComputedStyle(node).outlineWidth) > 0), true, "demo search has a visible keyboard focus ring");
        await query.press("Tab");
        assert.equal(await format.evaluate(node => node === document.activeElement), true, "Tab moves from empty search to the format selector");
        if (width === 375) {
          assert.equal(await page.locator(".picker-more").innerText(), "More");
          assert.equal(await page.locator(".picker-primary .picker-action-label").isVisible(), true);
          assert.equal(await page.locator(".picker-primary .key-caps").isVisible(), false);
          assert.equal(await page.locator("[data-picker] select").evaluate(node => node.getBoundingClientRect().height < 44), true, "format control stays slim");
          assert.equal(await page.locator(".picker-actions button").evaluateAll(nodes => nodes.every(node => node.getBoundingClientRect().height < 44)), true, "footer actions stay slim");
          const add = page.getByRole("button", { name: "Add", exact: true });
          await add.scrollIntoViewIfNeeded();
          const box = await add.boundingBox();
          await page.touchscreen.tap(box.x + box.width / 2, box.y - (44 - box.height) / 2 + 2);
          assert.equal(await page.getByRole("button", { name: "Remove", exact: true }).isVisible(), true, "invisible hit area activates Add");
          await page.getByRole("button", { name: "Remove", exact: true }).click();
        }
        await query.focus(); await query.press("ArrowDown");
        assert.equal(await page.locator(".picker-shelf code").innerText(), "[@fen2023clouds]", "arrow keys choose the next reference");
        for (const name of ["Add A field guide to clockwork clouds to citation", "Add Mapping the quiet islands of an invented sea to citation"]) {
          const tile = page.getByRole("button", { name, exact: true });
          if (width === 375) await tile.tap(); else await tile.click();
        }
        assert.equal(await page.locator(".picker-shelf code").innerText(), "[@fen2023clouds; @reed2025maps]", "tiles combine references in citation order");
        for (const name of ["Remove A field guide to clockwork clouds from citation", "Remove Mapping the quiet islands of an invented sea from citation"]) {
          await page.getByRole("button", { name, exact: true }).click();
        }
        await query.fill("not in this invented library");
        assert.equal(await page.getByText("No matching references", { exact: true }).isVisible(), true);
        assert.equal(await page.getByRole("button", { name: "Copy as Pandoc", exact: true }).isDisabled(), true);
        await query.fill("lantern");
        assert.equal(await page.getByRole("listbox", { name: "Best matches" }).isVisible(), true);
        assert.equal(await page.locator("[data-picker] .picker-panel").evaluate(node => node.getBoundingClientRect().height), panelHeight, "search keeps the three-row panel height");
        await query.press("Enter");
        assert.equal(await page.getByRole("button", { name: "Paste the copied citation into the sentence" }).evaluate(hasFocusRing), true, "keyboard Copy transfers its ring to Paste");
        await page.getByRole("button", { name: "Paste the copied citation into the sentence" }).click();
        assert.match(await page.locator("[data-result]").innerText(), /\[@vale2024lanterns\]/);
        await query.focus(); await query.press("Escape");
        assert.equal(await query.inputValue(), "");
        assert.equal(await page.locator(".picker-shelf").isVisible(), true);
        await query.press("Escape");
        assert.equal(await page.locator("[data-picker] .picker-panel").isVisible(), false);
        await page.getByRole("button", { name: "Open the picker again", exact: true }).click();
        assert.equal(await query.evaluate(hasFocusRing), false, "pointer reopen focuses search without a ring");
        assert.equal(await page.locator(".picker-shelf").isVisible(), true);
        await page.getByRole("tab", { name: "@typst", exact: true }).click();
        assert.equal(await page.getByRole("tab", { name: "@typst", exact: true }).evaluate(hasFocusRing), false, "pointer tabs leave no ring");
        await page.getByRole("tab", { name: "@typst", exact: true }).press("ArrowLeft");
        assert.equal(await page.getByRole("tab", { selected: true }).evaluate(hasFocusRing), true, "arrow-key tabs keep a ring");
        await page.getByRole("tab", { name: "@typst", exact: true }).click();
        assert.equal(await page.locator("[data-out]").innerText(), "@vale2024lanterns[p. 12]");
        assert.deepEqual(await page.getByRole("tab").allTextContents(), apps.map(app => `@${app.id}`));
        assert.deepEqual(await page.evaluate(() => document.getAnimations().filter(a => a.playState === "running").map(a => a.animationName)), []);
        const dark = await page.getByRole("switch").getAttribute("aria-checked");
        await page.getByRole("switch").click();
        chosenDark = await page.getByRole("switch").getAttribute("aria-checked");
        assert.notEqual(chosenDark, dark);
        assert.equal(await page.getByRole("switch").evaluate(node => {
          const track = node.querySelector(".appearance-track").getBoundingClientRect(), thumb = node.querySelector(".appearance-thumb").getBoundingClientRect();
          return thumb.left >= track.left && thumb.right <= track.right;
        }), true, "the theme thumb stays inside its track in both directions");
        await page.getByRole("switch").click();
        chosenDark = dark;
      }
      await page.getByRole("button", { name: "Use system" }).click();
      assert.equal(await page.getByRole("switch").getAttribute("aria-checked"), String(colorScheme === "dark"));
      assert.deepEqual(errors, []);
      await page.close();
    }
  } finally { await browser.close(); }
});
