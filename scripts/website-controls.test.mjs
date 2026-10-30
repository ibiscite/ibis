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
      assert.equal(await page.locator('.picker [role="option"]:visible .ref strong').innerText(), "Lantern gardens and the rhythm of imaginary cities");
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
      assert.equal(await page.locator("[data-result] code").innerText(), "[@vale2024lanterns]");
      await input.fill("no invented reference matches this");
      assert.equal(await page.locator(".picker-foot button").last().isDisabled(), true);
      assert.equal(await page.locator("[data-result] code").innerText(), "[@vale2024lanterns]");
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
      assert.equal(await staticPage.locator(".picker-shelf code").innerText(), "[@vale2024lanterns]");
      assert.equal(await staticPage.locator(".copied-notice").isVisible(), true);
      assert.equal(await staticPage.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
      assert.equal(await staticPage.locator("[data-download-privacy]:visible").count(), 4);
      await staticPage.close();
    }
  } finally { await browser.close(); }
});

test("picker header fits its container at 200% text and uses aligned themed gradient marks", { skip: !process.env.IBIS_WEBSITE_URL }, async () => {
  const url = new URL(process.env.IBIS_WEBSITE_URL);
  assert.equal(url.protocol, "http:"); assert.equal(url.hostname, "127.0.0.1");
  const { chromium } = await import(process.env.IBIS_PLAYWRIGHT_MODULE || "playwright");
  const browser = await chromium.launch({ executablePath: process.env.IBIS_CHROMIUM_EXECUTABLE });
  try {
    for (const fontSize of [16, 32]) for (const width of [320, 1180]) for (const colorScheme of ["light", "dark"]) for (const javaScriptEnabled of [false, true]) {
      const page = await browser.newPage({ viewport: { width, height: 900 }, colorScheme, javaScriptEnabled, reducedMotion: "reduce" });
      const session = await page.context().newCDPSession(page);
      await session.send("Page.setFontSizes", { fontSizes: { standard: fontSize, fixed: fontSize } });
      await page.goto(url.href + "#how");
      if (javaScriptEnabled) await page.getByRole("combobox", { name: "Search the made-up library" }).waitFor();
      const panel = page.locator("[data-picker] .picker-panel");
      // Exercise a narrow container on desktop too: viewport media queries alone
      // cannot protect a small picture inside the website's reading column.
      if (width === 1180) await panel.evaluate((node, size) => { node.style.width = `${size}px`; }, fontSize === 32 ? 240 : 280);
      const check = async (theme = colorScheme) => {
        const layout = await panel.evaluate(node => {
          const header = node.querySelector(".picker-search"), box = node.getBoundingClientRect(), band = header.getBoundingClientRect();
          const visible = [...header.querySelectorAll("input, button, select, .picker-query, .picker-format, .picker-mark, .picker-mark img")]
            .filter(child => child.getBoundingClientRect().width && child.getBoundingClientRect().height);
          const clipped = visible.filter(child => {
            const bounds = child.getBoundingClientRect();
            return bounds.left < Math.max(box.left, band.left) - 1 || bounds.right > Math.min(box.right, band.right) + 1
              || bounds.top < Math.max(box.top, band.top) - 1 || bounds.bottom > Math.min(box.bottom, band.bottom) + 1;
          }).map(child => child.className || child.tagName);
          const query = header.querySelector("input, .picker-query"), mark = visible.find(child => child.tagName === "IMG");
          const tile = node.querySelector(".tile").getBoundingClientRect(), title = node.querySelector(".ref").getBoundingClientRect();
          const image = mark?.getBoundingClientRect(), field = query.getBoundingClientRect();
          return { clipped, headerOverflow: header.scrollWidth > header.clientWidth, rootSize: parseFloat(getComputedStyle(document.documentElement).fontSize),
            queryWidth: field.width, querySize: parseFloat(getComputedStyle(query).fontSize), queryAlignment: field.left - title.left,
            markSource: mark?.getAttribute("src"), markWidth: image?.width, markAlignment: image && (image.left + image.width / 2) - (tile.left + tile.width / 2),
            mask: mark && getComputedStyle(mark.parentElement).maskImage };
        });
        const scenario = `${width}px viewport / ${fontSize}px font / ${colorScheme} / JS ${javaScriptEnabled}`;
        assert.equal(layout.rootSize, fontSize, scenario);
        assert.deepEqual(layout.clipped, [], `header controls must be entirely inside the panel: ${scenario}`);
        assert.equal(layout.headerOverflow, false, `overflow:hidden cannot conceal header overflow: ${scenario}`);
        assert.ok(layout.queryWidth >= 2 * layout.querySize, `the query must retain usable editing space: ${scenario}`);
        assert.ok(Math.abs(layout.queryAlignment) <= 1, `query and result titles stay aligned: ${scenario}`);
        assert.equal(layout.markSource, `assets/ibis-mark${theme === "dark" ? "-on-dark" : ""}.svg`, scenario);
        assert.equal(layout.markWidth, fontSize * 1.5, `the search mark is 24 px at normal text size: ${scenario}`);
        assert.ok(Math.abs(layout.markAlignment) <= 1, `the mark stays centred over the type tile: ${scenario}`);
        assert.equal(layout.mask, "none", `the gradient must remain intact: ${scenario}`);
      };
      await check();
      const override = colorScheme === "light" ? "dark" : "light";
      await page.evaluate(theme => { document.documentElement.dataset.theme = theme; }, override);
      await check(override);
      await page.evaluate(() => { delete document.documentElement.dataset.theme; });
      if (javaScriptEnabled) {
        const input = page.getByRole("combobox", { name: "Search the made-up library" });
        await input.fill(""); await check();
        await input.fill("lantern"); await page.getByRole("combobox", { name: "Copy format" }).selectOption("parencite"); await check();
      }
      await page.close();
    }
  } finally { await browser.close(); }
});

async function combineDemoReferences(page) {
  const input = page.getByRole("combobox", { name: "Search the made-up library" });
  await input.waitFor(); await input.fill("");
  for (let index = 0; index < 3; index++) {
    await input.press("Shift+Enter");
    if (index < 2) await input.press("ArrowDown");
  }
  await page.getByRole("combobox", { name: "Copy format" }).selectOption("parencite");
  await input.press("Enter");
}

// Let the receipt's observer and queued fit finish after a width change.
// Static pictures need only the forced layout in the assertions below.
const settlePicture = (page, interactive) => interactive
  ? page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))
  : Promise.resolve();

test("copied shelf and footer fit one row at 240 px and 200% text", { skip: !process.env.IBIS_WEBSITE_URL }, async () => {
  const url = new URL(process.env.IBIS_WEBSITE_URL);
  assert.equal(url.protocol, "http:"); assert.equal(url.hostname, "127.0.0.1");
  const { chromium } = await import(process.env.IBIS_PLAYWRIGHT_MODULE || "playwright");
  const browser = await chromium.launch({ executablePath: process.env.IBIS_CHROMIUM_EXECUTABLE });
  try {
    for (const fontSize of [16, 32]) for (const colorScheme of ["light", "dark"]) for (const javaScriptEnabled of [false, true]) for (const hasTouch of [false, true]) {
      const page = await browser.newPage({ viewport: { width: 1440, height: 1000 }, colorScheme, javaScriptEnabled, hasTouch, reducedMotion: "reduce" });
      const session = await page.context().newCDPSession(page);
      await session.send("Page.setFontSizes", { fontSizes: { standard: fontSize, fixed: fontSize } });
      await page.goto(url.href + "#how");
      if (javaScriptEnabled) await combineDemoReferences(page);
      const panel = page.locator("[data-picker] .picker-panel");
      for (const width of [720, 480, 240]) {
        await panel.evaluate((node, size) => { node.style.width = `${size}px`; }, width);
        await settlePicture(page, javaScriptEnabled);
        const layout = await panel.evaluate(node => {
          const shelf = node.querySelector(".picker-shelf"), foot = node.querySelector(".picker-foot"), citation = shelf.querySelector("code"), capsule = shelf.querySelector(".picker-status");
          const bounds = element => { const rect = element.getBoundingClientRect(); return { left: rect.left, right: rect.right, top: rect.top, bottom: rect.bottom, width: rect.width, height: rect.height }; };
          const elements = [...shelf.querySelectorAll(".picker-beak, code, .picker-status"), ...foot.querySelectorAll(".picker-library, button, .picker-static-control")];
          const clipped = elements.filter(element => {
            const rect = bounds(element), parent = bounds(element.closest(".picker-shelf, .picker-foot"));
            return rect.width && (rect.left < parent.left - 1 || rect.right > parent.right + 1 || rect.top < parent.top - 1 || rect.bottom > parent.bottom + 1);
          }).map(element => element.className || element.tagName);
          const actions = [...foot.querySelectorAll("button, .picker-static-control")];
          const statusText = foot.querySelector(".picker-library > span:last-child");
          return { clipped, shelfOverflow: shelf.scrollWidth > shelf.clientWidth, footOverflow: foot.scrollWidth > foot.clientWidth,
            capsuleWhole: capsule.scrollWidth <= capsule.clientWidth && getComputedStyle(capsule).whiteSpace === "nowrap",
            citationWidth: bounds(citation).width, citationEllipsis: getComputedStyle(citation).textOverflow, citationClipped: citation.scrollWidth > citation.clientWidth,
            actionRows: actions.map(action => bounds(action).top),
            labels: [...foot.querySelectorAll(".picker-action-label")].map(label => bounds(label).width),
            keysVisible: actions.every(action => [...action.querySelectorAll("kbd")].some(key => bounds(key).width > 0)),
            statusShortened: statusText.clientWidth < statusText.scrollWidth,
            actionNames: actions.map(action => action.getAttribute("aria-label") || action.textContent) };
        });
        const scenario = `${width}px / ${fontSize}px font / ${colorScheme} / JS ${javaScriptEnabled} / touch ${hasTouch}`;
        assert.deepEqual(layout.clipped, [], scenario);
        assert.equal(layout.shelfOverflow, false, `the shelf must fit internally: ${scenario}`);
        assert.equal(layout.footOverflow, false, `the footer must fit internally: ${scenario}`);
        assert.equal(layout.capsuleWhole, true, `Copied stays whole: ${scenario}`);
        assert.ok(layout.citationWidth > 0, `the citation retains room for its ellipsis: ${scenario}`);
        assert.equal(layout.citationEllipsis, "ellipsis", scenario);
        assert.ok(layout.actionRows.every(top => Math.abs(top - layout.actionRows[0]) < 1), `actions stay on one row: ${scenario}`);
        assert.equal(layout.keysVisible, true, `keys stay visible on touch as well: ${scenario}`);
        assert.ok(layout.actionNames.some(name => /Copy.*as/.test(name)), `compact actions keep their accessible names: ${scenario}`);
        if (width === 240) {
          assert.equal(layout.citationClipped, true, scenario);
          assert.ok(layout.labels.every(size => size <= 1), `narrow actions show keys only: ${scenario}`);
        }
        if (width === 480 && fontSize === 16 && javaScriptEnabled) {
          assert.ok(layout.labels.every(size => size > 1), `labels stay until the status has shortened: ${scenario}`);
          assert.equal(layout.statusShortened, true, scenario);
        }
      }
      await page.close();
    }
  } finally { await browser.close(); }
});

test("Copied notice fits at most two citation lines and keeps the frozen receipt", { skip: !process.env.IBIS_WEBSITE_URL }, async () => {
  const url = new URL(process.env.IBIS_WEBSITE_URL);
  assert.equal(url.protocol, "http:"); assert.equal(url.hostname, "127.0.0.1");
  const { chromium } = await import(process.env.IBIS_PLAYWRIGHT_MODULE || "playwright");
  const browser = await chromium.launch({ executablePath: process.env.IBIS_CHROMIUM_EXECUTABLE });
  try {
    for (const fontSize of [16, 32]) for (const javaScriptEnabled of [false, true]) {
      const page = await browser.newPage({ viewport: { width: 1440, height: 1000 }, javaScriptEnabled, reducedMotion: "reduce" });
      const session = await page.context().newCDPSession(page);
      await session.send("Page.setFontSizes", { fontSizes: { standard: fontSize, fixed: fontSize } });
      await page.goto(url.href + "#how");
      if (javaScriptEnabled) await combineDemoReferences(page);
      const receipt = page.locator(".picker-receipt"), citation = page.locator(".copied-notice code");
      const original = await citation.getAttribute("title");
      assert.equal(original, javaScriptEnabled ? "\\parencites{vale2024lanterns}{fen2023clouds}{reed2025maps}" : "[@vale2024lanterns]");
      const summaries = javaScriptEnabled ? [original, "\\parencites{vale2024lanterns}{fen2023clouds} +1 more", "\\parencite{vale2024lanterns} +2 more", "+3 more"] : [original, "+1 more"];
      if (javaScriptEnabled) {
        await page.getByRole("combobox", { name: "Search the made-up library" }).fill("not in this invented library");
        await page.getByRole("combobox", { name: "Copy format" }).selectOption("typst");
      }
      for (const width of [560, 240, 560]) {
        await receipt.evaluate((node, size) => { node.style.width = `${size}px`; }, width);
        await settlePicture(page, javaScriptEnabled);
        const measured = await citation.evaluate(node => {
          const visible = [...node.children].find(child => getComputedStyle(child).display !== "none");
          const rect = visible?.getBoundingClientRect();
          return { height: rect?.height, width: rect?.width, line: parseFloat(getComputedStyle(node).lineHeight),
            text: visible?.textContent, full: node.title, box: node.getBoundingClientRect().height, overflow: visible && visible.scrollWidth > visible.clientWidth };
        });
        assert.ok(measured.height > 0 && measured.height <= measured.line * 2 + 1, "the visible citation itself fits; a clipping parent cannot hide a third line");
        assert.ok(measured.box <= measured.line * 2 + 1);
        assert.equal(measured.overflow, false);
        assert.equal(measured.full, original, "later searches and formats never rewrite a copied receipt");
        assert.equal(await citation.getAttribute("aria-description"), original, "the full copied value remains accessible when visually summarised");
        assert.ok(summaries.includes(measured.text), "the visible receipt preserves the copied references and counts omissions");
        if (width === 240 && fontSize === 32) assert.match(measured.text, javaScriptEnabled ? /\+3 more/ : /\+1 more/, "the count includes every omitted reference");
        if (width === 560 && fontSize === 16) assert.doesNotMatch(measured.text, /\+\d+ more/, "widening restores the full receipt when it fits");
      }
      await page.close();
    }
  } finally { await browser.close(); }
});

test("RTL mirrors both citation beaks and preserves the Ibis logo and LTR keys", { skip: !process.env.IBIS_WEBSITE_URL }, async () => {
  const url = new URL(process.env.IBIS_WEBSITE_URL);
  assert.equal(url.protocol, "http:"); assert.equal(url.hostname, "127.0.0.1");
  const { chromium } = await import(process.env.IBIS_PLAYWRIGHT_MODULE || "playwright");
  const browser = await chromium.launch({ executablePath: process.env.IBIS_CHROMIUM_EXECUTABLE });
  try {
    for (const javaScriptEnabled of [false, true]) {
      const page = await browser.newPage({ viewport: { width: 1180, height: 900 }, javaScriptEnabled, reducedMotion: "reduce" });
      await page.goto(url.href + "#how");
      if (javaScriptEnabled) await combineDemoReferences(page);
      for (const dir of ["ltr", "rtl", "ltr"]) {
        await page.locator("[data-picker]").evaluate((node, direction) => { node.dir = direction; }, dir);
        const visual = await page.locator("[data-picker]").evaluate(node => {
          const style = element => { const css = getComputedStyle(element); return { scale: css.scale, rotate: css.rotate, transform: css.transform }; };
          const beaks = [...node.querySelectorAll(".picker-beak")];
          return { beaks: beaks.map(style), logo: style(node.querySelector(".picker-mark")), images: [...node.querySelectorAll(".picker-mark img")].map(style),
            direction: getComputedStyle(node.querySelector(".picker-shelf code")).direction,
            order: beaks.map(beak => beak.getBoundingClientRect().left) };
        });
        assert.equal(visual.beaks.length, 2);
        assert.ok(visual.beaks.every(beak => beak.scale === (dir === "rtl" ? "-1 1" : "none")));
        assert.equal(visual.beaks[1].rotate, "180deg");
        assert.deepEqual(visual.logo, { scale: "none", rotate: "none", transform: "none" });
        assert.ok(visual.images.every(image => image.scale === "none" && image.rotate === "none" && image.transform === "none"));
        assert.equal(visual.direction, "ltr");
        assert.equal(visual.order[0] > visual.order[1], dir === "rtl", "the mirrored pair follows the logical citation order");
      }
      await page.close();
    }
  } finally { await browser.close(); }
});
