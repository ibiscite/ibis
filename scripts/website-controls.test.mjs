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

test("navigation updates one current chip, scrolls only the chip bar, and keeps a followed link current", () => {
  const { ctx } = context();
  const names = ["how", "download", "setup", "roadmap", "why", "shape"], frames = [], handlers = {}, clicks = [];
  const links = names.map((name, index) => ({ attrs: {}, getAttribute: () => `#${name}`, hasAttribute(k) { return k in this.attrs; },
    setAttribute(k, v) { this.attrs[k] = v; }, removeAttribute(k) { delete this.attrs[k]; },
    getBoundingClientRect: () => ({ left: index * 100 - bar.scrollLeft, right: index * 100 + 80 - bar.scrollLeft }) }));
  const bar = { scrollLeft: 0, clientWidth: 200, scrollWidth: 600, addEventListener() {}, toggleAttribute() {}, getBoundingClientRect: () => ({ left: 0, right: 200 }) };
  ctx.window = { scrollY: 0, innerHeight: 1000, addEventListener: (name, fn) => { handlers[name] = fn; } };
  ctx.requestAnimationFrame = fn => frames.push(fn);
  ctx.document.documentElement.scrollHeight = 5400;
  ctx.document.addEventListener = (_name, fn) => clicks.push(fn);
  ctx.document.querySelectorAll = () => links;
  ctx.document.querySelector = selector => selector === ".index nav ul" ? bar : { id: selector.slice(1),
    getBoundingClientRect: () => { const top = names.indexOf(selector.slice(1)) * 1000 - ctx.window.scrollY; return { top, bottom: top + 900 }; } };
  const current = () => links.filter(link => link.hasAttribute("aria-current")).map(link => link.getAttribute());
  ctx.navigation(); assert.deepEqual(current(), ["#how"]); assert.equal(bar.scrollLeft, 0);
  ctx.window.scrollY = 3000; handlers.scroll(); handlers.scroll();
  assert.equal(frames.length, 1); frames.shift()();
  assert.deepEqual(current(), ["#roadmap"]); assert.equal(bar.scrollLeft, 196, "the chip bar scrolls sideways to reveal the current chip");
  ctx.window.scrollY = 4400; handlers.scroll(); frames.shift()(); assert.deepEqual(current(), ["#shape"], "the page end selects the last section");
  clicks.forEach(fn => fn({ target: { closest: () => links[4] } })); handlers.scroll(); frames.shift()();
  assert.deepEqual(current(), ["#why"], "a followed link stays current at the page end");
  handlers.wheel(); handlers.scroll(); frames.shift()(); assert.deepEqual(current(), ["#shape"], "scrolling releases the followed link");
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
  const { chromium } = await import(process.env.IBIS_PLAYWRIGHT_MODULE || "playwright");
  const browser = await chromium.launch({ executablePath: process.env.IBIS_CHROMIUM_EXECUTABLE });
  try {
    for (const width of [1180, 1440]) for (const colorScheme of ["light", "dark"]) {
      const page = await browser.newPage({ viewport: { width, height: 900 }, colorScheme, reducedMotion: "no-preference" });
      await page.goto(url.href);
      const contrast = await page.locator(".index .appearance").evaluate(group => {
        const rgba = color => { const context = document.createElement("canvas").getContext("2d"); context.fillStyle = color; context.fillRect(0, 0, 1, 1); return [...context.getImageData(0, 0, 1, 1).data]; };
        const over = (color, background) => { const [r, g, b, alpha] = rgba(color); return [r, g, b].map((v, i) => v * alpha / 255 + background[i] * (1 - alpha / 255)); };
        const luminance = channels => channels.map(v => v / 255).map(v => v <= .04045 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4).reduce((sum, v, i) => sum + v * [.2126, .7152, .0722][i], 0);
        const ratio = (a, b) => { a = luminance(a); b = luminance(b); return (Math.max(a, b) + .05) / (Math.min(a, b) + .05); };
        const background = rgba(getComputedStyle(document.body).backgroundColor).slice(0, 3), opacity = Number(getComputedStyle(group).opacity);
        const text = over(getComputedStyle(group).color, background).map((v, i) => v * opacity + background[i] * (1 - opacity));
        const track = over(getComputedStyle(group.querySelector(".appearance-track")).backgroundColor, background);
        return { text: ratio(text, background), switch: ratio(over(getComputedStyle(group.querySelector(".appearance-thumb")).backgroundColor, track), track) };
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
  } finally { await browser.close(); }
});

test("public demo supports keyboard copy/paste and remains visible in narrow and static layouts", { skip: !process.env.IBIS_WEBSITE_URL }, async () => {
  const url = new URL(process.env.IBIS_WEBSITE_URL);
  assert.equal(url.protocol, "http:"); assert.equal(url.hostname, "127.0.0.1");
  const { chromium } = await import(process.env.IBIS_PLAYWRIGHT_MODULE || "playwright");
  const browser = await chromium.launch({ executablePath: process.env.IBIS_CHROMIUM_EXECUTABLE });
  try {
    const surfaces = new Map();
    for (const width of [320, 390, 1180]) for (const colorScheme of ["light", "dark"]) {
      const page = await browser.newPage({ viewport: { width, height: 850 }, colorScheme, reducedMotion: "reduce" });
      const errors = []; page.on("pageerror", error => errors.push(error.message));
      await page.goto(url.href + "#how");
      const input = page.getByRole("combobox", { name: "Search the made-up library" });
      await input.waitFor();
      const panel = page.locator("[data-picker] .picker-panel");
      const frame = await panel.boundingBox();
      assert.equal(frame.height, 480, "the picker opens in its steady 480 px frame");
      const appearance = await panel.evaluate(node => ({ scheme: getComputedStyle(node).colorScheme, surface: getComputedStyle(node).backgroundColor }));
      assert.equal(appearance.scheme, "light dark");
      assert.notEqual(appearance.surface, "rgba(0, 0, 0, 0)");
      if (colorScheme === "light") surfaces.set(width, appearance.surface);
      else assert.notEqual(appearance.surface, surfaces.get(width), "system appearance changes the app surface");
      assert.equal(await page.locator(".picker-mark").isVisible(), true);
      const modifier = await page.evaluate(() => document.documentElement.dataset.platform === "macos" ? "Meta" : "Control");
      assert.equal(await input.inputValue(), "lantern");
      await input.fill("");
      assert.equal(await page.locator('.picker [role="option"]:visible').count(), 3);
      await input.press("ArrowDown");
      assert.match(await page.locator('[role="option"][aria-selected="true"]').innerText(), /clockwork clouds/);
      assert.equal(await input.getAttribute("aria-activedescendant"), await page.locator('[role="option"][aria-selected="true"]').getAttribute("id"));
      await input.press("Shift+Enter");
      assert.match(await page.locator(".picker-shelf code").innerText(), /fen2023clouds/);
      await input.press("Shift+Enter");
      await input.fill("lantern");
      assert.equal(await page.locator('.picker [role="option"]:visible').count(), 1);
      assert.match(await page.locator('.picker [role="option"]:visible').innerText(), /Sample · Lantern gardens/);
      const format = page.getByRole("combobox", { name: "Copy format" });
      await format.focus(); await format.selectOption("cite");
      assert.equal(await format.evaluate(node => node === document.activeElement), true, "changing the native format select preserves its keyboard focus");
      await format.selectOption("pandoc");
      await input.press("Enter");
      assert.equal(await page.locator(".copied-notice").isVisible(), true);
      assert.match(await page.locator(".copied-notice").innerText(), /Copied[\s\S]*vale2024lanterns/);
      assert.equal(await page.locator(".picker-beak-end").isVisible(), true);
      assert.equal((await panel.boundingBox()).height, frame.height, "copy feedback cannot resize the picker");
      const paste = page.getByRole("button", { name: "Paste the copied citation into the sentence" });
      await paste.press(`${modifier}+V`);
      assert.match(await page.locator("[data-result]").innerText(), /\[@vale2024lanterns\] \[Sample\]/);
      await input.fill("no invented reference matches this");
      assert.equal(await page.locator(".picker-foot button").last().isDisabled(), true);
      assert.match(await page.locator("[data-result]").innerText(), /\[@vale2024lanterns\] \[Sample\]/);
      assert.match(await page.locator(".picker-empty").innerText(), /No matching references/);
      assert.equal(await input.getAttribute("aria-activedescendant"), null);
      assert.equal(await input.getAttribute("aria-expanded"), "false");
      assert.equal(await page.locator(".picker-results").isVisible(), false, "the empty-state action lives outside the listbox");
      assert.equal(await page.locator(".picker-shelf").isVisible(), false, "no matches without a prepared citation has no shelf");
      assert.equal((await panel.boundingBox()).height, frame.height, "no matches cannot resize the picker");
      await input.press("Escape"); await input.press("Escape"); await input.press("Escape");
      await page.getByRole("button", { name: "Open the picker again" }).click();
      assert.equal((await panel.boundingBox()).height, frame.height, "reopening keeps the frame");
      for (const location of await page.locator(".intro, .platform:visible").all()) {
        const privacy = location.locator("[data-download-privacy]");
        assert.equal(await privacy.isVisible(), true);
        assert.match(await privacy.innerText(), /No account, no uploads, no telemetry; sources read without changes/);
        assert.equal(await privacy.getByRole("link", { name: "Privacy", exact: true }).getAttribute("href"), "https://github.com/ibiscite/ibis/blob/main/PRIVACY.md");
        assert.equal(await privacy.evaluate(node => node.scrollWidth <= node.clientWidth), true, "privacy sentence wraps at small widths");
      }
      await page.reload();
      assert.equal(await input.inputValue(), "lantern");
      assert.equal(await page.getByRole("button", { name: "Paste the copied citation into the sentence" }).count(), 0);
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true, "page never overflows sideways");
      assert.deepEqual(errors, []);
      await page.close();
    }
    for (const colorScheme of ["light", "dark"]) {
      const staticPage = await browser.newPage({ javaScriptEnabled: false, viewport: { width: 320, height: 850 }, colorScheme });
      await staticPage.goto(url.href + "#how");
      assert.equal(await staticPage.locator("[data-picker] .picker-panel").isVisible(), true);
      assert.match(await staticPage.locator("[data-picker]").innerText(), /Copy as Pandoc/);
      assert.match(await staticPage.locator("[data-picker]").innerText(), /\[@vale2024lanterns\] \[Sample\]/);
      assert.equal(await staticPage.locator(".copied-notice").isVisible(), true);
      assert.equal(await staticPage.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
      assert.equal(await staticPage.locator("[data-download-privacy]:visible").count(), 4);
      await staticPage.close();
    }
  } finally { await browser.close(); }
});
