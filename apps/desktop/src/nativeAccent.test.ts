import { afterEach, expect, test, vi } from "vitest";
import { setNativeAccent } from "./nativeAccent";
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

// The native accent's own block at the end of tokens.css.
const root = document.documentElement;
afterEach(() => setNativeAccent(null));

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
