export type Platform = "macos" | "windows" | "linux";

/** The actual system from `navigator.platform`; unknown systems use Linux conventions. */
export function platformOf(name: string): Platform {
  return /Mac|iPhone|iPad/.test(name) ? "macos" : /Win/.test(name) ? "windows" : "linux";
}

/** Shortcut labels and keys follow the platform: ⌘ on macOS, Ctrl elsewhere. */
export const isMac = typeof navigator !== "undefined" && platformOf(navigator.platform) === "macos";

/** Actual system facts stay independent of the shared macOS visual look. */
export function currentPlatform(): Platform {
  const set = typeof document === "undefined" ? undefined : document.documentElement.dataset.system;
  return set === "macos" || set === "windows" || set === "linux" ? set : platformOf(typeof navigator === "undefined" ? "" : navigator.platform);
}
