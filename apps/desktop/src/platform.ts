export type Platform = "macos" | "windows" | "linux";

/** The platform from `navigator.platform`; anything not macOS or Windows gets the GNOME look. */
export function platformOf(name: string): Platform {
  return /Mac|iPhone|iPad/.test(name) ? "macos" : /Win/.test(name) ? "windows" : "linux";
}

/** Shortcut labels and keys follow the platform: ⌘ on macOS, Ctrl elsewhere. */
export const isMac = typeof navigator !== "undefined" && platformOf(navigator.platform) === "macos";

/** The platform the picker is drawn for: the one set on the page, which the gallery can switch. */
export function currentPlatform(): Platform {
  const set = typeof document === "undefined" ? undefined : document.documentElement.dataset.platform;
  return set === "macos" || set === "windows" || set === "linux" ? set : platformOf(typeof navigator === "undefined" ? "" : navigator.platform);
}
