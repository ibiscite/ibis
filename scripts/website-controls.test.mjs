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
  const { loadPlaywright } = await import("./gallery-contact-sheet.mjs");
  const { chromium } = await loadPlaywright();
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
    const clear = page.getByRole("button", { name: "Clear search", exact: true });
    await clear.hover();
    assert.equal(await clear.evaluate(e => getComputedStyle(e).backgroundColor), "rgba(0, 0, 0, 0)", "clear remains a quiet cross on hover");
    await clear.click();
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
    await input.press("Enter"); await page.getByRole("button", { name: "Paste the copied citation into the sentence", exact: true }).click();
    assert.equal(await input.isVisible(), true, "picker stays visible after pasting");
    assert.equal((await page.locator("[data-result]").textContent()).match(/@/g).length, 6);
    assert.ok(await page.locator("[data-result]").evaluate(e => e.scrollWidth <= e.clientWidth + 1));
    assert.equal(await page.locator("[data-result]").evaluate(e => e === document.activeElement), true);
    const receipt = await page.locator("[data-result] code").textContent();
    await page.getByRole("combobox", { name: "Copy format" }).selectOption("cite"); await input.press("Enter");
    assert.equal(await page.locator("[data-result] code").textContent(), receipt, "copying again retains the pasted citation");
    const paste = page.getByRole("button", { name: "Paste the copied citation into the sentence", exact: true });
    await paste.press(mac ? "Meta+v" : "Control+v");
    assert.match(await page.locator("[data-result] code").textContent(), /^\\cite/);
    assert.equal(await page.locator("[data-result]").evaluate(e => e === document.activeElement), true, "keyboard paste restores focus");
    await input.press("Enter"); assert.equal(await input.evaluate(e => e === document.activeElement), true, "same-value copy keeps focus");
    assert.equal(await paste.count(), 0, "same-value copy needs no second paste");
    await input.dispatchEvent("keydown", previewKey);
    assert.equal(await page.locator(".picker-shelf code").isVisible(), false);
    assert.equal(await page.locator(".picker-status").isVisible(), true, "feedback survives a hidden output preview");
    assert.equal(await page.getByRole("status").textContent(), await page.locator(".picker-status").textContent(), "the always-rendered live region announces the same feedback");
    await page.emulateMedia({ forcedColors: "active" });
    assert.equal(await page.locator('.pick[aria-selected="true"]').evaluate(e => getComputedStyle(e).forcedColorAdjust), "none");
    await page.locator('.index nav a[href="#how"]').click();
    await page.waitForFunction(() => document.querySelector('.index nav a[href="#how"]').hasAttribute("aria-current"));
    assert.equal(await page.locator('.index a[aria-current] .key').evaluate(e => getComputedStyle(e).forcedColorAdjust), "none");
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    const touch = await browser.newPage({ viewport: { width: 320, height: 600 }, hasTouch: true, isMobile: true, reducedMotion: "reduce" });
    await touch.goto(url.href); const result = await touch.locator("[data-result]").evaluate(e => e.getBoundingClientRect().height);
    await touch.getByRole("button", { name: /^Copy as Pandoc/ }).click();
    await touch.locator(".paste-slot").evaluate(e => e.scrollIntoView({ block: "center" }));
    assert.ok(await touch.locator(".paste-slot").evaluate(e => { const r = e.getBoundingClientRect(), x = r.left + r.width / 2; return document.elementFromPoint(x, r.top - 6)?.closest(".paste-slot") === e && document.elementFromPoint(x, r.bottom + 6)?.closest(".paste-slot") === e; }), "paste remains a touch target");
    assert.equal(await touch.locator("[data-result]").evaluate(e => e.getBoundingClientRect().height), result, "showing the paste slot never moves the page");
    assert.ok(await touch.evaluate(() => { const h = e => document.querySelector(e).getBoundingClientRect().height; return Math.abs(h(".picker select") - h(".picker-foot button")) < 1; }), "the format menu and the footer buttons share one height");
    assert.ok(await touch.locator(".picker-format svg").isVisible(), "the format menu shows its own chevron in every browser");
    assert.match(await touch.locator(".demo-hint").textContent(), /^Tap a row/);
    assert.equal(await touch.locator(".picker-foot kbd").isVisible(), false);
    assert.ok(await touch.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    assert.equal(await touch.evaluate(() => { delete document.documentElement.dataset.platform; return getComputedStyle(document.documentElement).getPropertyValue("--selected-text").trim(); }), "#FFFFFF", "phones pair the selected row with white text, not WebKit's black AccentColorText");
    assert.ok(await touch.evaluate(() => { const logo = document.querySelector(".logo").getBoundingClientRect(), note = document.querySelector(".index-note").getBoundingClientRect(); return note.top < logo.bottom && note.left > logo.right; }), "the preview note shares the logo row");
    await touch.close();
  } finally { await browser.close(); }
});

test("logo ink aligns, the app shortcut opens the example and sharing uses the browser", { skip: !process.env.IBIS_WEBSITE_URL }, async () => {
  const { loadPlaywright } = await import("./gallery-contact-sheet.mjs");
  const { chromium } = await loadPlaywright();
  const browser = await chromium.launch({ executablePath: process.env.IBIS_CHROMIUM_EXECUTABLE });
  try {
    const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, reducedMotion: "reduce", permissions: ["clipboard-read", "clipboard-write"] });
    const page = await context.newPage();
    await page.addInitScript(() => { delete Navigator.prototype.share; Object.defineProperty(navigator, "platform", { get: () => "Linux x86_64" }); Object.defineProperty(navigator, "userAgentData", { get: () => ({ platform: "Linux" }) }); });
    await page.goto(process.env.IBIS_WEBSITE_URL);
    const [ink, chip] = await page.evaluate(() => {
      const logo = document.querySelector(".logo-light img, .logo img:not([hidden])").getBoundingClientRect();
      return [logo.right - logo.width * .0513, document.querySelector(".index nav .key").getBoundingClientRect().right];
    });
    assert.ok(Math.abs(ink - chip) <= 1.5, `wordmark ink ${ink} aligns with chips ${chip}`);
    assert.ok(await page.locator(".index-sticky").evaluate(e => e.scrollWidth <= e.clientWidth), "the sidebar never scrolls sideways");
    await page.locator(".demo-hint").waitFor();
    await page.keyboard.press("Control+Shift+Y");
    await page.waitForFunction(() => document.activeElement?.matches(".picker input"));
    assert.ok(await page.locator("[data-picker]").evaluate(e => { const r = e.getBoundingClientRect(); return r.top < innerHeight && r.bottom > 0; }), "the app shortcut brings the example into view");
    await page.keyboard.press("Enter"); await page.locator(".picker-status", { hasText: "Copied" }).waitFor();
    assert.equal(await page.locator(".picker-status").evaluate(e => e.textContent && getComputedStyle(e, "::before").content), '"✓"', "a copy confirms with a tick");
    await page.getByRole("button", { name: "Share Ibis" }).click();
    await page.locator(".share-status", { hasText: "Link copied" }).waitFor();
    assert.equal(await page.evaluate(() => navigator.clipboard.readText()), "https://ibiscite.org/");
    // Safari starts a text selection across the page when a sidebar link is dragged; Chromium drags the link instead.
    assert.equal(await page.locator(".index").evaluate(e => getComputedStyle(e).userSelect), "none", "dragging in the sidebar never selects page text");
    assert.equal(await page.locator(".closing .button").first().evaluate(e => getComputedStyle(e).height), "36px", "page buttons keep one slim height");
    const signature = page.locator(".signature .key");
    await signature.hover();
    await page.waitForFunction(() => document.querySelector(".signature .key").textContent === "[@diyanko]");
    await page.waitForTimeout(1100);
    assert.equal(await signature.textContent(), "[@diyanko]", "reduced motion holds the first citation form");
    await page.mouse.move(0, 0);
    assert.equal(await signature.textContent(), "@diyanko", "a chip returns to its key when the pointer leaves");
    await context.close();
  } finally { await browser.close(); }
});

test("the demo opens with the platform's Y shortcut across keyboard layouts", { skip: !process.env.IBIS_WEBSITE_URL }, async () => {
  const url = new URL(process.env.IBIS_WEBSITE_URL);
  assert.equal(url.protocol, "http:"); assert.equal(url.hostname, "127.0.0.1");
  const { loadPlaywright } = await import("./gallery-contact-sheet.mjs");
  const { chromium } = await loadPlaywright();
  const browser = await chromium.launch({ executablePath: process.env.IBIS_CHROMIUM_EXECUTABLE });
  try {
    for (const platform of ["MacIntel", "Win32", "Linux x86_64"]) {
      const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, reducedMotion: "reduce" });
      await page.addInitScript(platform => {
        Object.defineProperty(navigator, "platform", { get: () => platform });
        Object.defineProperty(navigator, "userAgentData", { get: () => ({ platform: platform === "MacIntel" ? "macOS" : platform === "Win32" ? "Windows" : "Linux" }) });
      }, platform);
      await page.goto(url.href); await page.locator(".demo-hint").waitFor();
      const mac = platform === "MacIntel", modifiers = { metaKey: mac, ctrlKey: !mac, shiftKey: true };
      assert.equal(await page.locator("html").getAttribute("data-platform"), mac ? "macos" : platform === "Win32" ? "windows" : "linux");
      const cases = [
        ...["KeyY", "KeyZ", "KeyT", "KeyO"].map(code => [{ key: "Y", code, ...modifiers }, true]),
        [{ key: "Z", code: "KeyY", ...modifiers }, false],
        [{ key: "Н", code: "KeyY", ...modifiers }, true],
        [{ key: "r", code: "KeyR", ctrlKey: true, metaKey: true }, false],
        [{ key: "Y", code: "KeyY", ...modifiers, repeat: true }, false],
        [{ key: "Y", code: "KeyY", ...modifiers, isComposing: true }, false],
      ];
      for (const [event, expected] of cases) {
        await page.locator('.index nav a[href="#how"]').focus();
        await page.evaluate(event => document.dispatchEvent(new KeyboardEvent("keydown", { ...event, bubbles: true, cancelable: true })), event);
        assert.equal(await page.locator(".picker input").evaluate(input => input === document.activeElement), expected, `${platform}: ${JSON.stringify(event)}`);
      }
      assert.match(await page.locator(".demo-hint").textContent(), mac ? /⇧⌘Y/ : /Ctrl\+Shift\+Y/);
      await page.close();
    }
  } finally { await browser.close(); }
});

test("browser captures, explicitly updates and removes locators across searches", { skip: !process.env.IBIS_WEBSITE_URL }, async () => {
  const { loadPlaywright } = await import("./gallery-contact-sheet.mjs");
  const { chromium } = await loadPlaywright();
  const browser = await chromium.launch({ executablePath: process.env.IBIS_CHROMIUM_EXECUTABLE });
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 700 }, reducedMotion: "reduce" });
    await page.goto(process.env.IBIS_WEBSITE_URL);
    const input = page.getByRole("combobox", { name: "Search the made-up library" }), output = page.locator(".picker-shelf code");
    await input.fill("okafor 2023 p. 12");
    assert.equal(await page.locator(".picker-locator bdi").textContent(), "Using page 12");
    await input.dispatchEvent("keydown", { key: "Enter", shiftKey: true, isComposing: true, bubbles: true });
    assert.equal(await page.locator(".tile.is-added").count(), 0, "composition never adds a reference");
    await input.press("Shift+Enter");
    await input.fill("nakamura ch. III"); await input.press("Shift+Enter");
    assert.equal(await output.textContent(), "[@okafor2023, p. 12; @nakamura2024, chap. III]");
    await input.fill("okafor 2023 p. 19");
    assert.equal(await output.textContent(), "[@okafor2023, p. 12; @nakamura2024, chap. III]");
    await input.press("Enter");
    await page.getByRole("button", { name: "Apply locator to okafor2023", exact: true }).click();
    assert.equal(await output.textContent(), "[@okafor2023, p. 19; @nakamura2024, chap. III]");
    await page.getByRole("button", { name: "Paste the copied citation into the sentence", exact: true }).click();
    assert.equal(await page.locator("[data-result] code").textContent(), "[@okafor2023, p. 12; @nakamura2024, chap. III]", "copy retains its captured contexts");
    await page.getByRole("button", { name: "Remove page 19 from okafor2023", exact: true }).click();
    assert.equal(await output.textContent(), "[@okafor2023; @nakamura2024, chap. III]");
    assert.equal(await page.locator('.tile[aria-pressed="true"]').count(), 2, "offscreen selected references stay selected");
    await input.press("Shift+Enter"); await input.press("Shift+Enter");
    assert.equal(await output.textContent(), "[@nakamura2024, chap. III; @okafor2023, p. 19]", "remove and readd captures the new locator at the end");
    await page.getByRole("button", { name: "Search as typed", exact: true }).click();
    assert.equal(await input.inputValue(), "okafor 2023 p. 19");
    assert.equal(await page.locator(".picker-empty").isVisible(), true);
    assert.equal(await output.textContent(), "[@nakamura2024, chap. III; @okafor2023, p. 19]", "literal search preserves captured contexts");
    await input.fill("okafor 2023 p. 20");
    assert.equal(await page.locator(".picker-locator bdi").textContent(), "Using page 20");
    await page.getByRole("combobox", { name: "Copy format" }).selectOption("keys");
    assert.equal(await output.textContent(), "nakamura2024, okafor2023");
    assert.match(await page.getByRole("status").textContent(), /bare keys cannot carry page numbers/);
    await page.close();
    const touch = await browser.newPage({ viewport: { width: 320, height: 650 }, hasTouch: true, isMobile: true, reducedMotion: "reduce" });
    await touch.goto(process.env.IBIS_WEBSITE_URL);
    await touch.locator(".picker-panel").evaluate(e => { e.dir = "rtl"; });
    const touchInput = touch.getByRole("combobox", { name: "Search the made-up library" });
    const value = `${"1, ".repeat(18)}2`;
    await touchInput.fill(`okafor pp ${value}`);
    await touch.getByRole("button", { name: "Add", exact: true }).click();
    const remove = touch.getByRole("button", { name: `Remove page ${value} from okafor2023`, exact: true });
    assert.equal(await remove.isVisible(), true);
    assert.ok(await touch.locator(".locator-token").evaluate(e => e.getBoundingClientRect().width <= 256 && e.scrollWidth <= e.clientWidth + 1), "long tokens stay bounded in RTL");
    assert.ok(await touch.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    await touch.emulateMedia({ forcedColors: "active" });
    await remove.click();
    assert.equal(await touch.locator(".tile.is-added").count(), 1);
    assert.equal(await touch.locator(".picker-shelf code").textContent(), "[@okafor2023]");
    await touch.close();
  } finally { await browser.close(); }
});
