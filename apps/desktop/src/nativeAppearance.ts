import { listen } from "@tauri-apps/api/event";
import { platformOf } from "./platform";
import { setNativeAccent, subscribeNativeAccent } from "./nativeAccent";

export { platformOf, type Platform } from "./platform";

type Dispose = () => void;

/** Exposes the platform to CSS for its search box, selected row and radius. */
export function setPlatform(name = typeof navigator === "undefined" ? "" : navigator.platform): void {
  document.documentElement.dataset.platform = platformOf(name);
}
type Subscribe = (changed: (value: unknown) => void) => Promise<Dispose>;

export function setNativeAppearance(value: unknown): void {
  document.documentElement.dataset.nativeAppearance = value === "mica" || value === "vibrancy" ? value : "opaque";
}

/** Subscribe before reading initial state; a newer preference event wins. */
export async function installNativeAppearance(load: () => Promise<unknown>, subscribe: Subscribe = (changed) =>
  subscribeNativeAccent().then(async (accentDispose) => {
    try {
      const dispose = await listen<unknown>("appearance:changed", (event) => changed(event.payload));
      return () => { dispose(); accentDispose(); };
    } catch (error) { accentDispose(); throw error; }
  })): Promise<Dispose> {
  setNativeAppearance("opaque");
  setNativeAccent(null);
  let changed = false;
  let active = true;
  let dispose: Dispose = () => {};
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
