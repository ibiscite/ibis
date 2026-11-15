import { platformOf } from "./platform";
import { setNativeAccent, subscribeNativeAccent } from "./nativeAccent";
import { events, type Source, type Stop } from "./nativeEvents";

/** One macOS look; actual system facts (such as the UI font) remain separate. */
export function setPlatform(name = typeof navigator === "undefined" ? "" : navigator.platform): void {
  document.documentElement.dataset.platform = "macos";
  document.documentElement.dataset.system = platformOf(name);
}

export function setNativeAppearance(value: unknown): void {
  document.documentElement.dataset.nativeAppearance = value === "acrylic" || value === "vibrancy" || value === "liquid-glass" ? value : "opaque";
}

/** Subscribe before reading initial state; a newer preference event wins. */
export async function installNativeAppearance(load: () => Promise<unknown>, subscribe: Source<(value: unknown) => void> = (changed) =>
  subscribeNativeAccent().then(async (accentDispose) => {
    try {
      const dispose = await events.onAppearanceChanged(changed);
      return () => { dispose(); accentDispose(); };
    } catch (error) { accentDispose(); throw error; }
  })): Promise<Stop> {
  setNativeAppearance("opaque");
  setNativeAccent(null);
  let changed = false;
  let active = true;
  let dispose: Stop = () => {};
  try {
    dispose = await subscribe((value) => { if (active) { changed = true; setNativeAppearance(value); } });
    const initial = await load();
    if (!changed) setNativeAppearance(initial);
  } catch {
    if (!changed) setNativeAppearance("opaque");
    console.error("ibis: native appearance is unavailable; using opaque surface");
  }
  return () => { active = false; dispose(); setNativeAppearance("opaque"); };
}
