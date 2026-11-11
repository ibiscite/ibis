import assert from "node:assert/strict";
import { readFile, mkdir } from "node:fs/promises";
import { runInNewContext } from "node:vm";
import { test } from "node:test";
import { join, resolve } from "node:path";
const script = await readFile(new URL("../website/site.js", import.meta.url), "utf8");
const apps = JSON.parse(await readFile(new URL("../website/content/writing-apps.json", import.meta.url), "utf8"));
const control = () => ({ attrs: {}, handlers: {}, setAttribute(k, v) { this.attrs[k] = v; },
  getAttribute(k) { return this.attrs[k]; }, hasAttribute(k) { return k in this.attrs; },
  removeAttribute(k) { delete this.attrs[k]; }, toggleAttribute(k, value) { if (value) this.attrs[k] = ""; else delete this.attrs[k]; },
  replaceChildren(...children) { this.children = children; },
  addEventListener(k, fn) { this.handlers[k] = fn; }, focus() { this.focused = true; } });
function context(choice = null, platform = "MacIntel", userAgent = "Desktop", maxTouchPoints = 0) {
  const stored = choice instanceof Map ? choice : new Map(choice ? [["ibis-theme", choice]] : []);
  const media = { matches: false, addEventListener(_name, fn) { this.change = fn; } };
  const ctx = { document: { documentElement: { dataset: {} }, addEventListener() {} },
    navigator: { platform, userAgent, maxTouchPoints },
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

test("Copy copies only the command and reports clipboard refusal", async () => {
  for (const allowed of [true, false]) {
    const label = { textContent: "Copy" };
    const { ctx } = context(), button = { ...control(), querySelector: () => label, dataset: { copy: 'ibis cite vale2024lanterns --locator "p. 12" --file sample.bib', copied: "Copied", failed: "Select to copy" } };
    const written = [], timers = [];
    ctx.navigator.clipboard = { async writeText(value) { if (!allowed) throw Error("blocked"); written.push(value); } };
    ctx.document.querySelectorAll = () => [button];
    ctx.setTimeout = fn => { timers.push(fn); };
    ctx.copyButtons(); await button.handlers.click();
    assert.deepEqual(written, allowed ? [button.dataset.copy] : []);
    assert.equal(label.textContent, allowed ? "Copied" : "Select to copy");
    timers[0](); assert.equal(label.textContent, "Copy");
  }
  const { ctx } = context(), button = control();
  ctx.document.querySelectorAll = () => [button]; ctx.copyButtons();
  assert.equal(button.hidden, true);
});

// Optional existing browser runtime. All requests are fulfilled from disk; no listener or network.
test("four routes fit phone and desktop in both themes; navigation, picker, Copy and reduced motion work", { skip: !process.env.IBIS_PLAYWRIGHT_MODULE }, async () => {
  const { chromium } = await import(process.env.IBIS_PLAYWRIGHT_MODULE);
  const browser = await chromium.launch({ executablePath: process.env.IBIS_CHROMIUM_EXECUTABLE });
  const root = resolve("website/dist"), screenshots = resolve(".local/website-review");
  await mkdir(screenshots, { recursive: true });
  try {
    for (const width of [375, 1440]) for (const colorScheme of ["light", "dark"]) {
      const page = await browser.newPage({ viewport: { width, height: 900 }, colorScheme, reducedMotion: "reduce" });
      const errors = []; page.on("pageerror", error => errors.push(error.message));
      await page.addInitScript(() => {
        Object.defineProperty(navigator, "clipboard", { value: { async writeText(value) { window.copiedCommand = value; } } });
      });
      await page.route("**/*", async route => {
        const url = new URL(route.request().url());
        if (url.hostname !== "ibis.test") return route.abort();
        const path = url.pathname.endsWith("/") ? `${url.pathname}index.html` : url.pathname;
        const type = path.endsWith(".html") ? "text/html" : path.endsWith(".css") ? "text/css" : /\.(m?js)$/.test(path) ? "text/javascript" : path.endsWith(".svg") ? "image/svg+xml" : undefined;
        await route.fulfill({ path: join(root, path), contentType: type, headers: path.endsWith(".html") ? { "content-security-policy": "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; manifest-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'" } : {} });
      });
      let chosenDark;
      for (const path of ["/", "/get/", "/today/", "/cli/"]) {
        await page.goto(`https://ibis.test${path}`);
        if (path === "/") await page.getByRole("combobox").waitFor();
        assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true, `${path} fits ${width} ${colorScheme}`);
        assert.equal(await page.getByRole("heading", { level: 1 }).count(), 1);
        assert.equal(await page.locator('.index nav a[aria-current]').count(), 1);
        if (chosenDark !== undefined) assert.equal(await page.getByRole("switch").getAttribute("aria-checked"), chosenDark);
        if (path === "/get/") {
          assert.equal(await page.locator('.platform a[href*="/releases/download/"]:visible').count(), 5);
          assert.equal(await page.getByRole("heading", { name: "Choose your library", exact: true }).isVisible(), true);
          assert.equal(await page.getByRole("heading", { name: "Press your shortcut", exact: true }).isVisible(), true);
          assert.equal(await page.locator("details").count(), 0);
        }
        if (path === "/cli/") for (const button of await page.getByRole("button", { name: "Copy", exact: true }).all()) {
          const command = await button.getAttribute("data-copy");
          await button.click();
          assert.equal(await page.evaluate(() => window.copiedCommand), command);
          assert.equal(await button.innerText(), "Copied");
        }
        if (path !== "/") continue;
        assert.equal(await page.locator(".picker-primary").isVisible(), true);
        assert.equal(await page.locator(".picker-library").isVisible(), true);
        const query = page.getByRole("combobox");
        await query.fill("not in this invented library");
        assert.equal(await page.getByText("No matching references", { exact: true }).isVisible(), true);
        assert.equal(await page.getByRole("button", { name: "Copy as Pandoc", exact: true }).isDisabled(), true);
        await query.fill("lantern"); await query.press("Enter");
        await page.getByRole("button", { name: "Paste the copied citation into the sentence" }).click();
        assert.match(await page.locator("[data-result]").innerText(), /\[@vale2024lanterns\]/);
        await page.getByRole("tab", { name: "@typst", exact: true }).click();
        assert.equal(await page.locator("[data-out]").innerText(), "@vale2024lanterns[p. 12]");
        assert.deepEqual(await page.getByRole("tab").allTextContents(), apps.map(app => `@${app.id}`));
        assert.deepEqual(await page.evaluate(() => document.getAnimations().filter(a => a.playState === "running").map(a => a.animationName)), []);
        const dark = await page.getByRole("switch").getAttribute("aria-checked");
        await page.getByRole("switch").click();
        chosenDark = await page.getByRole("switch").getAttribute("aria-checked");
        assert.notEqual(chosenDark, dark);
        await page.screenshot({ path: join(screenshots, `home-${width}-${colorScheme}.png`), fullPage: true });
      }
      await page.getByRole("button", { name: "Use system" }).click();
      assert.equal(await page.getByRole("switch").getAttribute("aria-checked"), String(colorScheme === "dark"));
      assert.deepEqual(errors, []);
      await page.close();
    }
  } finally { await browser.close(); }
});
