#!/usr/bin/env node
// Browser contact sheet of the picker states gallery for pull request evidence:
// every look (macOS, Windows, Linux) × theme (light, dark) × state, in normal
// and larger text, rendered from one machine in Playwright's Chromium.
//
//   node scripts/gallery-contact-sheet.mjs [--output DIR] [--dist DIR] [--no-build]
//       [--state ID]... [--platform macos|windows|linux]... [--theme light|dark]...
//       [--text normal|large]... [--scale N]
//
// It builds the evidence gallery (vite --mode evidence) unless --no-build, serves
// it on 127.0.0.1 only, and writes one PNG per frame plus contact-sheet-<text>.png.
// These are browser renders: system fonts, materials (vibrancy, Mica) and window
// chrome are not the native ones; CI's native captures cover those.
// The pointer rests outside the window, so frames show the picker at rest; the
// pin-hover frame puts it on the pin. High contrast and forced colours are the
// browser's own emulation, not a CSS copy (the native evidence app keeps that).
// Needs Playwright with its Chromium already installed (it never downloads one).
// Exit codes: 0 every frame rendered, 1 a frame failed, 2 Playwright or Chromium unavailable.
import { execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const LOOKS = { platform: ["macos", "windows", "linux"], theme: ["light", "dark"], text: ["normal", "large"] };
const NAMES = { macos: "macOS", windows: "Windows", linux: "Linux", light: "Light", dark: "Dark", normal: "Normal text", large: "Larger text" };
// As each system's webview reports it; must match PLATFORM_NAMES in gallery-fixtures.ts.
export const PLATFORM_NAMES = { macos: "MacIntel", windows: "Win32", linux: "Linux x86_64" };
export const VIEWPORT = { width: 720, height: 480 }; // the picker window (tauri.conf.json)
// Frames the catalog cannot describe: they need the browser's pointer or media emulation.
const VARIANTS = [
  { id: "pin-hover", state: "pinned", label: "Pointer on the footer unpin", hover: ".pin-button" },
  { id: "forced-colors", state: "high-contrast", label: "Forced colours (emulated)", media: { forcedColors: "active" } },
];
const MEDIA = { "high-contrast": { contrast: "more" } };
// A frame that has not rendered by then is failed; the 50-reference preview
// takes several seconds on a busy machine.
const FRAME_TIMEOUT = 60_000;
const TYPES = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".svg": "image/svg+xml",
  ".png": "image/png", ".woff2": "font/woff2", ".json": "application/json", ".ico": "image/x-icon" };

export class Unavailable extends Error {}

function parse(argv) {
  const options = { output: path.join(repo, "target", "gallery-contact-sheet"), dist: path.join(repo, "apps", "desktop", "dist"),
    build: true, scale: 1, state: [], platform: [], theme: [], text: [] };
  for (let i = 0; i < argv.length; i++) {
    const flag = argv[i];
    const value = () => {
      const next = argv[++i];
      if (next === undefined) throw new Error(`${flag} needs a value`);
      return next;
    };
    if (flag === "--output") options.output = path.resolve(value());
    else if (flag === "--dist") options.dist = path.resolve(value());
    else if (flag === "--no-build") options.build = false;
    else if (flag === "--scale") options.scale = Number(value());
    else if (flag === "--state") options.state.push(value());
    else if (flag.slice(2) in LOOKS) {
      const name = flag.slice(2);
      const chosen = value();
      if (!LOOKS[name].includes(chosen)) throw new Error(`${flag} is one of ${LOOKS[name].join(", ")}`);
      options[name].push(chosen);
    } else throw new Error(`unknown option ${flag}; see the comment at the top of this script`);
  }
  if (!Number.isFinite(options.scale) || options.scale < 1 || options.scale > 3) throw new Error("--scale is between 1 and 3");
  for (const name of Object.keys(LOOKS)) if (options[name].length === 0) options[name] = LOOKS[name];
  return options;
}

export async function loadPlaywright() {
  try {
    return await import("playwright");
  } catch {
    // A global install (npm install -g playwright) is not visible to import().
    const require = createRequire(import.meta.url);
    let globalRoot = [];
    try {
      globalRoot = [execFileSync("npm", ["root", "-g"], { encoding: "utf8", shell: process.platform === "win32" }).trim()];
    } catch { /* no npm: only NODE_PATH is left */ }
    const paths = [...(process.env.NODE_PATH ?? "").split(path.delimiter).filter(Boolean), ...globalRoot];
    try {
      return require(require.resolve("playwright", { paths }));
    } catch {
      throw new Unavailable("Playwright is not installed; install it with its Chromium (npx playwright install chromium) and run again.");
    }
  }
}

/** Static files from the built gallery, on loopback only, never outside `dist`. */
export function serve(dist) {
  const server = createServer(async (request, response) => {
    let file = "";
    try {
      file = path.resolve(dist, `.${decodeURIComponent(new URL(request.url ?? "/", "http://127.0.0.1").pathname)}`);
    } catch { /* a malformed path is answered like a missing file */ }
    if (request.method !== "GET" || !file.startsWith(dist + path.sep)) {
      response.writeHead(404).end();
      return;
    }
    try {
      const body = await readFile(file);
      response.writeHead(200, { "content-type": TYPES[path.extname(file)] ?? "application/octet-stream" }).end(body);
    } catch {
      response.writeHead(404).end();
    }
  });
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => resolve(server));
  });
}

function source() {
  try {
    const sha = execFileSync("git", ["rev-parse", "--short=12", "HEAD"], { cwd: repo, encoding: "utf8" }).trim();
    const dirty = execFileSync("git", ["status", "--porcelain"], { cwd: repo, encoding: "utf8" }).trim() !== "";
    return dirty ? `${sha} with uncommitted changes` : sha;
  } catch {
    return "unknown commit";
  }
}

const escape = (text) => text.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

async function contactSheet(browser, output, text, columns, states, frames, label) {
  const cell = (state, column) => {
    const frame = frames.get(`${text}/${column.platform}-${column.theme}/${state.id}`);
    return frame?.png ? `<td><img src="data:image/png;base64,${frame.png.toString("base64")}" alt=""></td>`
      : `<td class="failed">Not rendered</td>`;
  };
  const html = `<!doctype html><meta charset="utf-8"><style>
    body { margin: 16px; background: #d9d9de; color: #1c1c1f; font: 13px/1.4 system-ui, sans-serif; }
    h1 { font-size: 16px; margin: 0 0 12px; } table { border-spacing: 8px; }
    th { font-weight: 600; text-align: left; } th[scope=row] { width: 9em; vertical-align: top; }
    img { display: block; width: 360px; height: 240px; border: 1px solid #8b8b95; border-radius: 6px; }
    .failed { width: 360px; height: 240px; text-align: center; color: #b3261e; border: 1px dashed #b3261e; }
  </style>
  <h1>${escape(label)} · ${escape(NAMES[text])}</h1>
  <table><tr><th></th>${columns.map((c) => `<th scope="col">${NAMES[c.platform]} · ${NAMES[c.theme]}</th>`).join("")}</tr>
  ${states.map((state) => `<tr><th scope="row">${escape(state.label)}<br><code>${escape(state.id)}</code></th>${columns.map((c) => cell(state, c)).join("")}</tr>`).join("\n")}
  </table>`;
  const page = await browser.newPage({ viewport: { width: 1200, height: 800 } });
  try {
    await page.setContent(html, { waitUntil: "load" });
    await page.screenshot({ path: path.join(output, `contact-sheet-${text}.png`), fullPage: true });
  } finally {
    await page.close();
  }
}

/** Builds the evidence gallery unless `build` is false, then starts Chromium and the loopback server. */
export async function openGallery({ build, dist }) {
  const { chromium } = await loadPlaywright();
  if (build) {
    execFileSync("pnpm", ["exec", "vite", "build", "--mode", "evidence"], { cwd: path.join(repo, "apps", "desktop"), stdio: "inherit", shell: process.platform === "win32" });
  }
  dist = path.resolve(dist);
  try {
    await readFile(path.join(dist, "gallery.html"));
  } catch {
    throw new Error(`${dist} has no gallery.html; build it with: pnpm -C apps/desktop exec vite build --mode evidence`);
  }
  let browser;
  try {
    browser = await chromium.launch();
  } catch (error) {
    throw new Unavailable(`Playwright's Chromium could not start: ${error.message.split("\n")[0]}`);
  }
  try {
    return { browser, server: await serve(dist) };
  } catch (error) {
    await browser.close();
    throw error;
  }
}

async function main() {
  const options = parse(process.argv.slice(2));
  const { browser, server } = await openGallery(options);
  const base = `http://127.0.0.1:${server.address().port}`;
  const failures = [];
  try {
    // The catalog page lists every state, so this script never keeps its own copy.
    const catalog = await browser.newPage();
    await catalog.goto(`${base}/gallery.html`);
    const listed = await catalog.$$eval(".gallery-grid h2 a", (links) => links.map((link) => ({
      id: new URL(link.href).searchParams.get("state"), label: link.textContent ?? "" })));
    await catalog.close();
    if (listed.length === 0) throw new Error("the gallery lists no states");
    let states = [...listed.map((state) => ({ ...state, state: state.id, media: MEDIA[state.id] })),
      ...VARIANTS.filter((variant) => listed.some((state) => state.id === variant.state))];
    const unknown = options.state.filter((id) => !states.some((state) => state.id === id));
    if (unknown.length > 0) throw new Error(`unknown state ${unknown.join(", ")}`);
    if (options.state.length > 0) states = states.filter((state) => options.state.includes(state.id));

    const output = path.resolve(options.output);
    // Remove only this script's earlier results, never anything else in --output.
    for (const text of LOOKS.text) {
      await rm(path.join(output, text), { recursive: true, force: true });
      await rm(path.join(output, `contact-sheet-${text}.png`), { force: true });
    }
    await rm(path.join(output, "manifest.json"), { force: true });
    const columns = options.platform.flatMap((platform) => options.theme.map((theme) => ({ platform, theme })));
    const jobs = options.text.flatMap((text) => columns.flatMap((column) => states.map((state) => ({ text, ...column, state }))));
    const frames = new Map();
    const contexts = new Map();
    for (const { theme } of columns) {
      if (!contexts.has(theme)) {
        contexts.set(theme, await browser.newContext({ viewport: VIEWPORT, deviceScaleFactor: options.scale, colorScheme: theme, reducedMotion: "reduce" }));
      }
    }
    const render = async (job, timeout) => {
      const name = `${job.text}/${job.platform}-${job.theme}/${job.state.id}`;
      const dir = path.join(output, job.text, `${job.platform}-${job.theme}`);
      await mkdir(dir, { recursive: true });
      const page = await contexts.get(job.theme).newPage();
      if (job.state.media) await page.emulateMedia(job.state.media);
      // Headless Chromium starts the pointer at (0, 0), inside the picker, so
      // every :hover rule (the pin, the first row) would show in a rest frame.
      await page.mouse.move(-1, -1);
      // The bundled picker reads the platform for its key labels (⌘ or Ctrl) before
      // the gallery's own ?platform= switch can run, so report it from the start.
      await page.addInitScript((platform) => {
        Object.defineProperty(navigator, "platform", { configurable: true, get: () => platform });
      }, PLATFORM_NAMES[job.platform]);
      const errors = [];
      page.on("pageerror", (error) => errors.push(error.message));
      page.on("console", (message) => { if (message.type() === "error") errors.push(message.text()); });
      const query = new URLSearchParams({ state: job.state.state, platform: job.platform, theme: job.theme, text: job.text });
      try {
        await page.goto(`${base}/gallery.html?${query}`);
        await page.waitForFunction((id) => document.documentElement.dataset.galleryRendered === id, job.state.state, { timeout });
        if (job.state.hover) await page.hover(job.state.hover, { timeout: 5_000 });
        // Let hover fades finish; endless ones (a spinner) never do and are skipped,
        // and a paused one is waited for at most two seconds.
        await page.evaluate(() => Promise.race([new Promise((resolve) => setTimeout(resolve, 2_000)), Promise.all(document.getAnimations()
          .filter((animation) => animation.effect?.getComputedTiming().endTime !== Infinity)
          .map((animation) => animation.finished.catch(() => undefined)))]));
        if (errors.length > 0) throw new Error(errors.join("; "));
        const png = await page.screenshot({ path: path.join(dir, `${job.state.id}.png`) });
        frames.set(name, { png });
        return null;
      } catch (error) {
        await page.screenshot({ path: path.join(dir, `failure-${job.state.id}.png`) }).catch(() => undefined);
        return `${name}: ${error.message.split("\n")[0]}`;
      } finally {
        await page.close();
      }
    };
    let next = 0;
    const retry = [];
    const worker = async () => {
      while (next < jobs.length) {
        const job = jobs[next++];
        if (await render(job, FRAME_TIMEOUT)) retry.push(job);
      }
    };
    await Promise.all(Array.from({ length: 4 }, worker));
    // A frame that failed beside others gets one more try on its own, so CPU
    // contention never decides what the evidence shows; a real failure fails twice.
    for (const job of retry) {
      const failure = await render(job, FRAME_TIMEOUT);
      if (failure) failures.push(failure);
      else await rm(path.join(output, job.text, `${job.platform}-${job.theme}`, `failure-${job.state.id}.png`), { force: true });
    }
    const browserName = `Chromium ${browser.version()}`;
    const label = `Ibis picker states, ${source()}. Browser render in ${browserName}, synthetic data; not a native capture`;
    for (const text of options.text) await contactSheet(browser, output, text, columns, states, frames, label);
    await writeFile(path.join(output, "manifest.json"), `${JSON.stringify({ source: source(), browser: browserName,
      viewport: VIEWPORT, scale: options.scale, rendered: [...frames.keys()].sort(), failed: failures }, null, 2)}\n`);
    console.log(`${frames.size} of ${jobs.length} frames rendered into ${output}`);
  } finally {
    server.close();
    await browser.close();
  }
  if (failures.length > 0) {
    console.error(`Not rendered:\n  ${failures.join("\n  ")}`);
    process.exitCode = 1;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = error instanceof Unavailable ? 2 : 1;
  });
}
