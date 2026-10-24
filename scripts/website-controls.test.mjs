import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { runInNewContext } from "node:vm";
import { test } from "node:test";
const script = await readFile(new URL("../website/site.js", import.meta.url), "utf8");
function context(choice = null) {
  const stored = new Map(choice ? [["ibis-theme", choice]] : []);
  const media = { matches: false, addEventListener(_name, fn) { this.change = fn; } };
  const ctx = { document: { documentElement: { dataset: {} }, addEventListener() {} },
    navigator: { platform: "MacIntel", userAgent: "Desktop", maxTouchPoints: 0 },
    localStorage: { getItem: key => stored.get(key), setItem: (key, value) => stored.set(key, value), removeItem: key => stored.delete(key) },
    matchMedia: () => media };
  runInNewContext(script, ctx);
  return { ctx, media, stored };
}
const control = () => ({ attrs: {}, handlers: {}, setAttribute(k, v) { this.attrs[k] = v; },
  addEventListener(k, fn) { this.handlers[k] = fn; }, focus() { this.focused = true; } });

test("appearance is applied before paint; both controls sync, persist and reset focus", () => {
  const { ctx, media, stored } = context("light");
  assert.equal(ctx.document.documentElement.dataset.theme, "light");
  assert.equal(ctx.document.documentElement.dataset.platform, "macos");
  const groups = Array.from({ length: 2 }, () => {
    const toggle = control(), reset = control();
    return { toggle, reset, hidden: true, querySelector: selector => selector.includes("switch") ? toggle : reset };
  });
  ctx.document.querySelectorAll = () => groups; ctx.appearance();
  groups[0].toggle.handlers.click();
  assert.equal(stored.get("ibis-theme"), "dark");
  for (const group of groups) { assert.equal(group.hidden, false); assert.equal(group.toggle.attrs["aria-checked"], "true"); }
  groups[1].reset.handlers.click();
  assert.equal(stored.has("ibis-theme"), false);
  assert.equal(groups[1].toggle.focused, true);
  media.matches = true; media.change();
  assert.equal(groups[0].toggle.attrs["aria-checked"], "true");
  assert.equal(context("untrusted").ctx.document.documentElement.dataset.theme, undefined);
});

test("navigation updates one current chip and scrolls it only when the section changes", () => {
  const { ctx, media } = context(); media.matches = true;
  const names = ["how", "download", "setup", "roadmap", "why", "shape"], frames = [], handlers = {};
  const calls = [];
  const links = names.map(name => ({ attrs: {}, getAttribute: () => `#${name}`, hasAttribute(k) { return k in this.attrs; },
    setAttribute(k, v) { this.attrs[k] = v; }, removeAttribute(k) { delete this.attrs[k]; }, querySelector: () => ({ scrollIntoView: args => calls.push([name, args]) }) }));
  const bar = { scrollLeft: 0, clientWidth: 200, scrollWidth: 600, addEventListener() {}, toggleAttribute() {} };
  ctx.window = { scrollY: 0, innerHeight: 1000, addEventListener: (name, fn) => { handlers[name] = fn; } };
  ctx.requestAnimationFrame = fn => frames.push(fn);
  ctx.document.documentElement.scrollHeight = 7000;
  ctx.document.querySelectorAll = () => links;
  ctx.document.querySelector = selector => selector === ".index nav ul" ? bar : { getBoundingClientRect: () => ({ top: names.indexOf(selector.slice(1)) * 1000 - ctx.window.scrollY }) };
  ctx.navigation(); assert.equal(calls.length, 1);
  ctx.window.scrollY = 1000; handlers.scroll(); handlers.scroll();
  assert.equal(frames.length, 1); frames.shift()();
  assert.equal(calls.at(-1)[0], "download");
  assert.equal(calls.at(-1)[1].inline, "nearest"); assert.equal(calls.at(-1)[1].block, "nearest");
  ctx.window.scrollY = 1010; handlers.scroll(); frames.shift()(); assert.equal(calls.length, 2);
  ctx.window.scrollY = 6000; handlers.scroll(); frames.shift()(); assert.equal(calls.at(-1)[0], "shape");
  assert.equal(links.filter(link => link.hasAttribute("aria-current")).length, 1);
});

test("resizing into a tablet or phone restores all platform downloads", () => {
  const { ctx, media } = context();
  const blocks = ["mac", "windows", "linux"].map(os => ({ dataset: { os }, hidden: false, before() {} }));
  const cta = {}, choice = { replaceChildren(...items) { this.items = items; } };
  ctx.document.querySelectorAll = () => blocks;
  ctx.document.querySelector = () => cta;
  ctx.document.createElement = tag => tag === "p" ? choice : control();
  ctx.platforms(); assert.deepEqual(blocks.map(block => block.hidden), [false, true, true]);
  choice.items[3].handlers.click();
  assert.equal(cta.textContent, "Get Ibis for Linux");
  media.matches = true; media.change();
  assert.deepEqual(blocks.map(block => block.hidden), [false, false, false]); assert.equal(choice.hidden, true);
  assert.equal(cta.textContent, "Get the early preview");
  media.matches = false; media.change(); assert.deepEqual(blocks.map(block => block.hidden), [true, true, false]);
  assert.equal(cta.textContent, "Get Ibis for Linux", "explicit platform choice survives a resize");
});

test("desktop sidebar remains clickable over every scrolled section", { skip: !process.env.IBIS_WEBSITE_URL }, async () => {
  const url = new URL(process.env.IBIS_WEBSITE_URL);
  assert.equal(url.protocol, "http:"); assert.equal(url.hostname, "127.0.0.1");
  const { loadPlaywright } = await import("./gallery-contact-sheet.mjs");
  const { chromium } = await loadPlaywright();
  const browser = await chromium.launch({ executablePath: process.env.IBIS_CHROMIUM_EXECUTABLE });
  try {
    for (const width of [1180, 1440]) for (const colorScheme of ["light", "dark"]) {
      const page = await browser.newPage({ viewport: { width, height: 900 }, colorScheme, reducedMotion: "no-preference" });
      await page.goto(url.href);
      const contrast = await page.locator(".index .appearance").evaluate(group => {
        const rgb = color => color.match(/[\d.]+/g).slice(0, 3).map(Number);
        const luminance = channels => channels.map(v => v / 255).map(v => v <= .04045 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4).reduce((sum, v, i) => sum + v * [.2126, .7152, .0722][i], 0);
        const ratio = (a, b) => { a = luminance(a); b = luminance(b); return (Math.max(a, b) + .05) / (Math.min(a, b) + .05); };
        const background = rgb(getComputedStyle(document.body).backgroundColor), opacity = Number(getComputedStyle(group).opacity);
        const text = rgb(getComputedStyle(group).color).map((v, i) => v * opacity + background[i] * (1 - opacity));
        return { text: ratio(text, background), switch: ratio(rgb(getComputedStyle(group.querySelector(".appearance-thumb")).backgroundColor), rgb(getComputedStyle(group.querySelector(".appearance-track")).backgroundColor)) };
      });
      assert.ok(contrast.text >= 4.5, "resting theme labels remain readable");
      assert.ok(contrast.switch >= 3, "theme switch thumb remains distinct");
      for (const id of ["setup", "shape", "how", "roadmap", "download"]) {
        await page.locator(`.index nav a[href="#${id}"]`).click({ timeout: 1500 });
        await page.waitForFunction(id => location.hash === `#${id}`, id, { timeout: 1500 });
        await page.waitForFunction(id => {
          const top = document.querySelector(`#${id}`).getBoundingClientRect().top;
          return top >= 0 && top < innerHeight / 2;
        }, id, { timeout: 1500 });
      }
      await page.locator('.index nav a[href="#how"]').press("Enter");
      await page.waitForFunction(() => location.hash === "#how");
      await page.close();
    }
    for (const during of [false, true]) {
    const page = await browser.newPage({ viewport: { width: 1440, height: 360 }, reducedMotion: "no-preference" });
    let release;
    const held = new Promise(resolve => { release = resolve; });
    await page.route("**/demo.js", async route => { await held; await route.continue(); });
    await page.goto(url.href);
    await page.locator('.index nav a[href="#shape"]').click();
    if (during) await page.waitForFunction(() => scrollY > 100 && document.querySelector("#shape").getBoundingClientRect().top > innerHeight);
    else await page.waitForFunction(() => Math.abs(document.querySelector("#shape").getBoundingClientRect().top - 24) < 1);
    release();
    await page.getByRole("combobox", { name: "Search the made-up library" }).waitFor();
    await page.waitForFunction(() => Math.abs(document.querySelector("#shape").getBoundingClientRect().top - 24) < 2);
    assert.equal(await page.locator('.index nav a[href="#shape"]').getAttribute("aria-current"), "location", "late enhancement preserves the current section");
    await page.close();
    }
  } finally { await browser.close(); }
});

test("browser keeps navigation, keyboard selection and long output visible", { skip: !process.env.IBIS_WEBSITE_URL }, async () => {
  const url = new URL(process.env.IBIS_WEBSITE_URL);
  assert.equal(url.protocol, "http:"); assert.equal(url.hostname, "127.0.0.1");
  const { loadPlaywright } = await import("./gallery-contact-sheet.mjs");
  const { chromium } = await loadPlaywright();
  const browser = await chromium.launch({ executablePath: process.env.IBIS_CHROMIUM_EXECUTABLE });
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 600 }, colorScheme: "light", reducedMotion: "reduce" });
    await page.goto(url.href);
    const input = page.getByRole("combobox", { name: "Search the made-up library" }); await input.waitFor();
    const mac = await page.locator("html").getAttribute("data-platform") === "macos";
    await page.getByRole("button", { name: "Clear search", exact: true }).click();
    assert.equal(await input.inputValue(), "");
    await page.locator(".pick").nth(0).click({ modifiers: [mac ? "Meta" : "Control"] });
    await page.locator(".pick .tile").nth(1).click();
    assert.equal((await page.locator(".picker-shelf code").textContent()).match(/@/g).length, 2, "modifier-click and tile click combine references");
    const fill = await page.locator(".pick .tile").nth(0).evaluate(e => getComputedStyle(e).backgroundColor);
    await page.locator(".pick").nth(1).click(); await page.locator(".pick .tile").nth(0).hover();
    assert.equal(await page.locator(".pick .tile").nth(0).evaluate(e => getComputedStyle(e).backgroundColor), fill, "added tile retains its contrasting fill on hover");
    await page.locator(".pick .tile").nth(0).click(); await page.locator(".pick .tile").nth(1).click();
    assert.equal(await page.locator('.tile[aria-pressed="true"]').count(), 0);
    assert.match(await page.locator(".logo-light img").evaluate(e => e.currentSrc), /\/ibis-logo\.svg$/);
    await page.mouse.move(300, 300); await page.mouse.wheel(0, 1200); await page.waitForTimeout(100);
    const before = await page.evaluate(() => scrollY);
    await page.mouse.wheel(0, 750); await page.waitForTimeout(100);
    assert.equal(Math.round(await page.evaluate(() => scrollY) - before), 750, "chip updates preserve ordinary scrolling");
    await input.fill(""); await input.press("ArrowDown");
    assert.notEqual(await input.evaluate(e => getComputedStyle(e).outlineStyle), "none", "keyboard focus remains visible");
    await input.press("Home");
    const previewKey = { key: "i", ctrlKey: !mac, metaKey: mac, bubbles: true };
    await input.dispatchEvent("keydown", previewKey);
    assert.equal(await page.locator(".picker-shelf").isVisible(), false);
    await input.dispatchEvent("keydown", { ...previewKey, repeat: true });
    assert.equal(await page.locator(".picker-shelf").isVisible(), false, "held preview shortcut toggles once");
    await input.dispatchEvent("keydown", previewKey);
    // Start at the first result again, then add every invented reference.
    await input.fill("x"); await input.fill("");
    for (let i = 0; i < 6; i++) {
      await input.press("Shift+Enter");
      await input.dispatchEvent("keydown", { key: "Enter", shiftKey: true, repeat: true, bubbles: true });
      assert.equal(await page.locator(".tile.is-added").count(), i + 1, "held Enter never toggles a reference twice");
      if (i < 5) await input.press("ArrowDown");
    }
    const fits = await page.locator('.pick[aria-selected="true"]').evaluate(e => {
      const row = e.getBoundingClientRect(), list = e.closest(".picker-results").getBoundingClientRect();
      return row.top >= list.top - 1 && row.bottom <= list.bottom + 1;
    });
    assert.ok(fits, "keyboard-selected result stays inside the list");
    assert.ok(await page.locator(".picker-shelf").evaluate(e => e.scrollWidth <= e.clientWidth + 1));
    await input.press("Enter"); await page.getByRole("button", { name: "Paste the copied citation into this example", exact: true }).click();
    assert.equal(await input.isVisible(), true, "picker stays visible after pasting");
    assert.equal((await page.locator("[data-result]").textContent()).match(/@/g).length, 6);
    assert.ok(await page.locator("[data-result]").evaluate(e => e.scrollWidth <= e.clientWidth + 1));
    await page.emulateMedia({ forcedColors: "active" });
    assert.equal(await page.locator('.pick[aria-selected="true"]').evaluate(e => getComputedStyle(e).forcedColorAdjust), "none");
    await page.locator('.index nav a[href="#how"]').click();
    await page.waitForFunction(() => document.querySelector('.index nav a[href="#how"]').hasAttribute("aria-current"));
    assert.equal(await page.locator('.index a[aria-current] .key').evaluate(e => getComputedStyle(e).forcedColorAdjust), "none");
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
  } finally { await browser.close(); }
});
