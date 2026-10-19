import { listen } from "@tauri-apps/api/event";

type Dispose = () => void;
type Subscribe = (changed: (value: unknown) => void) => Promise<Dispose>;

export function setNativeAppearance(value: unknown): void {
  document.documentElement.dataset.nativeAppearance = value === "mica" || value === "vibrancy" ? value : "opaque";
}

/** Subscribe before reading initial state; a newer preference event wins. */
export async function installNativeAppearance(load: () => Promise<unknown>, subscribe: Subscribe = (changed) =>
  listen<unknown>("appearance:changed", (event) => changed(event.payload))): Promise<Dispose> {
  setNativeAppearance("opaque");
  let changed = false;
  let dispose: Dispose = () => {};
  try {
    dispose = await subscribe((value) => { changed = true; setNativeAppearance(value); });
    const initial = await load();
    if (!changed) setNativeAppearance(initial);
  } catch {
    setNativeAppearance("opaque");
    console.error("ibis: native appearance is unavailable; using opaque surface");
  }
  return () => { dispose(); setNativeAppearance("opaque"); };
}
