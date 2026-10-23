import { listen } from "@tauri-apps/api/event";
import "./nativeAccent.css";

function onColor(rgb: number[]): string {
  const linear = rgb.map((c) => c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4);
  const l = linear[0]! * .2126 + linear[1]! * .7152 + linear[2]! * .0722;
  return (l + .05) / .05 >= 1.05 / (l + .05) ? "#000" : "#fff";
}

/** Only bounded, opaque sRGB reaches CSS; public roles remain in the cascade. */
export function setNativeAccent(value: unknown): void {
  const root = document.documentElement;
  const rgb = Array.isArray(value) && value.length === 3
    && Array.from(value).every((c) => typeof c === "number" && Number.isFinite(c) && c >= 0 && c <= 1) ? value as number[] : null;
  for (const name of ["fill", "on", "light", "dark", "on-hover-light", "on-hover-dark", "on-pressed-light", "on-pressed-dark"]) root.style.removeProperty(`--native-accent-${name}`);
  delete root.dataset.nativeAccent;
  if (!rgb) return;
  root.style.setProperty("--native-accent-fill", `color(srgb ${rgb.join(" ")})`);
  root.style.setProperty("--native-accent-on", onColor(rgb));
  // Match the macOS neutral roles in tokens.css; tests read those roles directly.
  for (const [theme, ink] of [["light", [22, 24, 29]], ["dark", [245, 245, 247]]] as const) {
    for (const [state, weight] of [["hover", .92], ["pressed", .84]] as const) {
      root.style.setProperty(`--native-accent-on-${state}-${theme}`, onColor(rgb.map((c, i) => c * weight + ink[i]! / 255 * (1 - weight))));
    }
  }
  // Retain the hue at the endpoints; the fill itself is never adjusted.
  for (const [theme, base, scale] of [["light", 0, .12], ["dark", .96, .04]] as const) {
    root.style.setProperty(`--native-accent-${theme}`, `color(srgb ${rgb.map((c) => base + c * scale).join(" ")})`);
  }
  root.dataset.nativeAccent = "true";
}

export async function subscribeNativeAccent(): Promise<() => void> {
  let active = true;
  const dispose = await listen<unknown>("accent:changed", ({ payload }) => { if (active) setNativeAccent(payload); });
  return () => { active = false; dispose(); setNativeAccent(null); };
}
