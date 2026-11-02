import { afterEach, expect, test, vi } from "vitest";
import { setNativeAccent, subscribeNativeAccent } from "./nativeAccent";
import { listen, type Event } from "@tauri-apps/api/event";
import { installNativeAppearance } from "./nativeAppearance";
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
const css = readFileSync(new FileURL("./nativeAccent.css", import.meta.url), "utf8");
const root = document.documentElement;
afterEach(() => setNativeAccent(null));

test("native sRGB preserves red, yellow, blue and boundary fills with readable pairs", () => {
  for (const rgb of [[1, 0, 0], [1, .72, 0], [0, .48, 1], [0, 0, 0], [1, 1, 1]]) {
    setNativeAccent(rgb);
    expect(root.dataset.nativeAccent).toBe("true");
    expect(root.style.getPropertyValue("--native-accent-fill")).toBe(`color(srgb ${rgb.join(" ")})`);
    const linear = rgb.map((c) => c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4);
    const l = linear[0]! * .2126 + linear[1]! * .7152 + linear[2]! * .0722;
    const black = root.style.getPropertyValue("--native-accent-on") === "#000";
    expect(black ? (l + .05) / .05 : 1.05 / (l + .05)).toBeGreaterThanOrEqual(3);
    expect(root.style.getPropertyValue("--accent")).toBe("");
  }
});

test("invalid and unavailable input clears every private native input", () => {
  for (const bad of [null, undefined, {}, "red", [], Array(3), [0, 0], [0, 0, 0, 1], [NaN, 0, 0], [Infinity, 0, 0], [-.01, 0, 0], [0, 1.01, 0], ["0", 0, 0]]) {
    setNativeAccent([1, 1, 0]);
    setNativeAccent(bad);
    expect(root.hasAttribute("data-native-accent")).toBe(false);
    for (const name of ["fill", "on", "light", "dark"]) expect(root.style.getPropertyValue(`--native-accent-${name}`)).toBe("");
  }
});

test("accent text and focus keep contrast on the real window, field, menu and raised surfaces", () => {
  const luminance = (rgb: number[]) => rgb.reduce((sum, c, i) => sum + (c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4) * [.2126, .7152, .0722][i]!, 0);
  for (const rgb of [[1, 0, 0], [1, .72, 0], [0, .48, 1], [0, 0, 0], [1, 1, 1]]) {
    setNativeAccent(rgb);
    for (const [theme, surfaces] of [
      ["light", ["fcfcfd", "ffffff", "f6f6f8", "e5e5e7", "fafafb"]],
      ["dark", ["242426", "2c2c2f", "28282c", "36363a", "1d1d20"]],
    ] as const) {
      const text = root.style.getPropertyValue(`--native-accent-${theme}`).slice(11, -1).split(" ").map(Number);
      for (const hex of surfaces) {
        const surface = hex.match(/../g)!.map((c) => parseInt(c, 16) / 255);
        const values = [luminance(text), luminance(surface)].sort((a, b) => a - b);
        expect((values[1]! + .05) / (values[0]! + .05)).toBeGreaterThanOrEqual(4.5); // also exceeds the focus cue's 3:1
      }
    }
  }
});

test("subscription disposal clears inputs and rejects queued accent events", async () => {
  let changed!: (event: Event<unknown>) => void;
  const unlisten = vi.fn();
  vi.mocked(listen).mockImplementation(async (_name, handler) => { changed = handler; return unlisten; });
  const dispose = await subscribeNativeAccent();
  changed({ event: "accent:changed", id: 0, payload: [1, 1, 0] });
  expect(root.dataset.nativeAccent).toBe("true");
  dispose();
  changed({ event: "accent:changed", id: 0, payload: [1, 0, 0] });
  expect(unlisten).toHaveBeenCalledOnce();
  expect(root.hasAttribute("data-native-accent")).toBe(false);
});

test("both subscriptions precede the replay query and accent survives a failed read", async () => {
  const handlers = new Map<string, (event: Event<unknown>) => void>();
  vi.mocked(listen).mockImplementation(async (name, handler) => { handlers.set(name, handler); return () => {}; });
  const dispose = await installNativeAppearance(async () => {
    expect([...handlers.keys()]).toEqual(["accent:changed", "appearance:changed"]);
    handlers.get("accent:changed")!({ event: "accent:changed", id: 0, payload: [1, 1, 0] });
    throw new Error("synthetic");
  });
  expect(root.dataset.nativeAccent).toBe("true");
  dispose();
  expect(root.hasAttribute("data-native-accent")).toBe(false);
});

test("the native accent adds no row rule of its own: the shared selected-row tile covers it (D9)", () => {
  expect(css).not.toMatch(/data-citation-selected/);
});

test("selected rows and menu highlights keep WebKit's selection pair on the native accent", () => {
  // Selection is deeper than the accent and follows it (docs/design.md, Accent and selection).
  expect(css).toMatch(/@supports \(background: -apple-system-selected-content-background\) and \(color: -apple-system-alternate-selected-text\)\s*\{\s*:root\[data-native-accent="true"\]\[data-system="macos"\]:not\(\.notice-window\)\s*\{\s*--selected: -apple-system-selected-content-background;\s*--selected-text: -apple-system-alternate-selected-text;/);
  const bridge = css.indexOf("--selected: var(--accent); --selected-text: var(--text-on-accent)");
  expect(bridge).toBeGreaterThan(-1);
  expect(css.indexOf("--selected: -apple-system-selected-content-background")).toBeGreaterThan(bridge);
  setNativeAccent([0, 122 / 255, 1]);
  expect(root.style.getPropertyValue("--native-accent-on")).toBe("#fff");
  setNativeAccent([1, .78, 0]);
  expect(root.style.getPropertyValue("--native-accent-on")).toBe("#000");
});

test("accent text stays the accent's own colour, moved only as far as contrast needs", () => {
  setNativeAccent([0, 122 / 255, 1]);
  const [lightRed, lightGreen, lightBlue] = root.style.getPropertyValue("--native-accent-light").slice(11, -1).split(" ").map(Number);
  // Darkening keeps the hue exactly; the ring reads as blue, not near-black.
  expect(lightRed).toBe(0);
  expect(lightGreen! / lightBlue!).toBeCloseTo(122 / 255);
  expect(lightBlue).toBeGreaterThan(.7);
  const [darkRed, , darkBlue] = root.style.getPropertyValue("--native-accent-dark").slice(11, -1).split(" ").map(Number);
  expect(darkBlue).toBe(1);
  expect(darkRed).toBeLessThan(.6);
});

test("the focus ring follows the Mac's accent", () => {
  expect(css).toMatch(/--ring: color-mix\(in srgb, var\(--accent\) 50%, transparent\);/);
});

test("a primary button keeps one readable foreground through hover and press", () => {
  // Hover and press mix the fill with black (tokens.css), so the foreground never flips.
  expect(css).not.toMatch(/on-hover|on-pressed|\.ui-button/);
  const luminance = (rgb: number[]) => rgb.reduce((l, c, i) => l + (c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4) * [.2126, .7152, .0722][i]!, 0);
  for (const rgb of [[.4, .55, .5], [0, 122 / 255, 1], [247 / 255, 79 / 255, 158 / 255], [140 / 255, 140 / 255, 140 / 255], [.6, .4, 0], [1, 0, 0], [0, 0, 0], [1, 1, 1]]) {
    setNativeAccent(rgb);
    const on = root.style.getPropertyValue("--native-accent-on");
    for (const weight of [1, .9, .8]) {
      const l = luminance(rgb.map((c) => c * weight));
      expect(on === "#000" ? (l + .05) / .05 : 1.05 / (l + .05)).toBeGreaterThanOrEqual(3);
    }
  }
});
