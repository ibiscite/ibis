import { listen } from "@tauri-apps/api/event";

function luminance(rgb: number[]): number {
  const linear = rgb.map((c) => c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4);
  return linear[0]! * .2126 + linear[1]! * .7152 + linear[2]! * .0722;
}

/** White, as macOS draws text on its accent, unless the accent is too bright for it. */
function onColor(rgb: number[]): string {
  return 1.05 / (luminance(rgb) + .05) >= 3 ? "#fff" : "#000";
}

/** The accent itself, mixed toward black or white only as far as 4.5:1 needs. */
function accentText(rgb: number[], toward: number, surface: number): number[] {
  for (let weight = 1; weight > 0; weight -= .01) {
    const mixed = rgb.map((c) => c * weight + toward * (1 - weight));
    const [hi, lo] = [luminance(mixed), surface].sort((a, b) => b - a);
    if ((hi! + .05) / (lo! + .05) >= 4.5) return mixed;
  }
  return [toward, toward, toward];
}

/** Only bounded, opaque sRGB reaches CSS; public roles remain in the cascade. */
export function setNativeAccent(value: unknown): void {
  const root = document.documentElement;
  const rgb = Array.isArray(value) && value.length === 3
    && Array.from(value).every((c) => typeof c === "number" && Number.isFinite(c) && c >= 0 && c <= 1) ? value as number[] : null;
  for (const name of ["fill", "on", "light", "dark"]) root.style.removeProperty(`--native-accent-${name}`);
  delete root.dataset.nativeAccent;
  delete root.dataset.nativeAccentDeep;
  if (!rgb) return;
  root.style.setProperty("--native-accent-fill", `color(srgb ${rgb.join(" ")})`);
  root.style.setProperty("--native-accent-on", onColor(rgb));
  // Readable on each mode's hardest surface (a pressed fill); the fill itself is never adjusted.
  for (const [theme, toward, surface] of [["light", 0, .78], ["dark", 1, .04]] as const) {
    root.style.setProperty(`--native-accent-${theme}`, `color(srgb ${accentText(rgb, toward, surface).join(" ")})`);
  }
  // Apple's blue (4.02:1) and red keep their measured fill; paler accents go deeper.
  if (1.05 / (luminance(rgb) + .05) < 4) root.dataset.nativeAccentDeep = "true";
  root.dataset.nativeAccent = "true";
}

export async function subscribeNativeAccent(): Promise<() => void> {
  let active = true;
  const dispose = await listen<unknown>("accent:changed", ({ payload }) => { if (active) setNativeAccent(payload); });
  return () => { active = false; dispose(); setNativeAccent(null); };
}
