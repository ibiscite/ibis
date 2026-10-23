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
    expect(black ? (l + .05) / .05 : 1.05 / (l + .05)).toBeGreaterThanOrEqual(4.5);
    expect(root.style.getPropertyValue("--accent")).toBe("");
  }
});

test("invalid and unavailable input clears every private native input", () => {
  for (const bad of [null, undefined, {}, "red", [], Array(3), [0, 0], [0, 0, 0, 1], [NaN, 0, 0], [Infinity, 0, 0], [-.01, 0, 0], [0, 1.01, 0], ["0", 0, 0]]) {
    setNativeAccent([1, 1, 0]);
    setNativeAccent(bad);
    expect(root.hasAttribute("data-native-accent")).toBe(false);
    for (const name of ["fill", "on", "light", "dark", "on-hover-light", "on-hover-dark", "on-pressed-light", "on-pressed-dark"]) expect(root.style.getPropertyValue(`--native-accent-${name}`)).toBe("");
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

test("an added tile on the native accent uses its paired text fill, keeping scarlet off the row fill", () => {
  expect(css).toMatch(/:root\[data-native-accent="true"\]\[data-platform="macos"\][^{}]*\[data-citation-selected\][^{}]*\.result-tile\s*\{[^}]*background:\s*var\(--text-on-accent\)[^}]*color:\s*var\(--citation-tile\)/);
});

test("native primary foregrounds stay readable through hover and press in both themes", () => {
  const tokens = readFileSync(new FileURL("./tokens.css", import.meta.url), "utf8");
  const neutrals = tokens.match(/:root\[data-platform="macos"\][^{]*\{[^}]*--text:\s*light-dark\((#[0-9a-f]+), (#[0-9a-f]+)\)/)!;
  const luminance = (rgb: number[]) => rgb.reduce((l, c, i) => l + (c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4) * [.2126, .7152, .0722][i]!, 0);
  for (const rgb of [[.4, .55, .5], [0, 122 / 255, 1], [119 / 255, 119 / 255, 119 / 255], [.6, .4, 0], [1, 0, 0], [0, 0, 0], [1, 1, 1]]) {
    setNativeAccent(rgb);
    for (const [theme, hex] of [["light", neutrals[1]!], ["dark", neutrals[2]!]] as const) {
      const ink = hex.slice(1).match(/../g)!.map((c) => parseInt(c, 16) / 255);
      for (const [state, weight] of [["hover", .92], ["pressed", .84]] as const) {
        const on = root.style.getPropertyValue(`--native-accent-on-${state}-${theme}`);
        expect(on).toMatch(/^#(?:000|fff)$/);
        const l = luminance(rgb.map((c, i) => c * weight + ink[i]! * (1 - weight)));
        expect(on === "#000" ? (l + .05) / .05 : 1.05 / (l + .05)).toBeGreaterThanOrEqual(4.5);
      }
    }
  }
});
