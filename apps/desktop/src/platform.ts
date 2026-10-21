export type Platform = "macos" | "windows" | "linux";

/** The platform from `navigator.platform`; anything not macOS or Windows gets the GNOME look. */
export function platformOf(name: string): Platform {
  return /Mac|iPhone|iPad/.test(name) ? "macos" : /Win/.test(name) ? "windows" : "linux";
}

/** Shortcut labels and keys follow the platform: ⌘ on macOS, Ctrl elsewhere. */
export const isMac = typeof navigator !== "undefined" && platformOf(navigator.platform) === "macos";
