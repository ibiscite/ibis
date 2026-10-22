import { isTauri } from "@tauri-apps/api/core";
import type { Copied, Density, LibraryStatus, SearchResult } from "./api";
import type { Sources } from "./bindings";
import type { PickerApi, PickerPreview } from "./App";
import type { Platform } from "./nativeAppearance";

// All names and references on this page are synthetic. Providers never call IPC.
type GalleryLibrary = LibraryStatus;
export type GalleryState = {
  id: string;
  label: string;
  library: GalleryLibrary;
  sources?: Sources;
  results?: SearchResult[];
  query?: string;
  copied?: Copied;
  theme?: "light" | "dark" | "contrast";
  direction?: "ltr" | "rtl";
  largeText?: boolean;
  /** The Copied notice shown after the picker hides, drawn over the picker. */
  notice?: string;
  /** Row density from the picker's saved preferences. */
  density?: Density;
  /** The picker's own Larger text setting (×1.15), unlike `largeText`, which doubles the system text size. */
  largerTextSetting?: boolean;
  /** Seeded once the picker has rendered: pin it, open More, choose this many results, open All keys. */
  pinned?: boolean;
  menuOpen?: boolean;
  /** Opens the header format menu; a string is typed into its filter. */
  formatMenu?: true | string;
  tray?: number;
  formatted?: boolean;
  keys?: boolean;
};

/**
 * Review switches for a browser render of the gallery (board M):
 * `?platform=macos|windows|linux`, `?theme=light|dark`, `?text=normal|large`
 * and `?density=compact|comfortable|spacious`. Anything else, a repeated
 * switch or the native evidence app (which must show its real system) gets
 * no switch at all.
 */
export type GallerySwitches = {
  platform?: Platform;
  theme?: "light" | "dark";
  text?: "normal" | "large";
  density?: Density;
};

const SWITCH_VALUES = {
  platform: ["macos", "windows", "linux"],
  theme: ["light", "dark"],
  text: ["normal", "large"],
  density: ["compact", "comfortable", "spacious"],
} as const;

export function gallerySwitches(search: string, native = isTauri()): GallerySwitches {
  if (native) return {};
  const params = new URLSearchParams(search);
  const switches: Record<string, string> = {};
  for (const [name, allowed] of Object.entries(SWITCH_VALUES)) {
    const values = params.getAll(name);
    if (values.length === 1 && (allowed as readonly string[]).includes(values[0]!)) switches[name] = values[0]!;
  }
  return switches as GallerySwitches;
}

/** A state's own setting wins over a switch, so "Dark mode" or "Compact rows" always show what they say. */
export function withSwitches(state: GalleryState, switches: GallerySwitches): GalleryState {
  return {
    ...state,
    theme: state.theme ?? switches.theme,
    density: state.density ?? switches.density,
    largerTextSetting: state.largerTextSetting ?? (switches.text === undefined ? undefined : switches.text === "large"),
  };
}

// What each system's webview reports; the picker's look and key labels follow it.
const PLATFORM_NAMES: Record<Platform, string> = { macos: "MacIntel", windows: "Win32", linux: "Linux x86_64" };

export function applyPlatformSwitch(platform: Platform, target: Navigator = navigator): void {
  Object.defineProperty(target, "platform", { configurable: true, get: () => PLATFORM_NAMES[platform] });
}

// Sets the look (data-platform), the Copied notice and, in the development
// server, the key labels: gallery.tsx imports this module before App.tsx. A
// production bundle evaluates the shared App chunk first, so there the labels
// (⌘ or Ctrl) follow the browser's own system; the contact-sheet script reports
// the platform from the first script on, so its labels match the look.
if (typeof window !== "undefined") {
  const { platform } = gallerySwitches(window.location.search);
  if (platform) applyPlatformSwitch(platform);
}

function library(overrides: Partial<GalleryLibrary> = {}): GalleryLibrary {
  return { revision: "gallery", state: "loaded", name: "example.bib", snapshot_name: "example.bib", has_snapshot: true, zotero: false, zotero_libraries: [],
    references: 24, source_counts: (overrides.references ?? 24) > 0 ? { "synthetic-gallery": overrides.references ?? 24 } : {}, warnings: 0, error: null, note: null, settings_notice: null,
    snapshot_zotero: false, stale: false, updated_at: null, checked_at: hoursAgo(3), ...overrides };
}

/** Times relative to the page load, so captures say the same thing on every run. */
function hoursAgo(hours: number) {
  return Date.now() - hours * 3_600_000;
}

function reference(key: string, title: string, authors = "Example Author", container: string | null = null, itemType = "article-journal"): SearchResult {
  const names = authors.split(", ");
  return { revision: "gallery", id: { source: "synthetic-gallery", native_key: key }, key, title, authors, year: 2026, matched: ["title"],
    first_author: names[0] ?? "", author_count: names.length, container, item_type: itemType };
}

// Every type icon appears in the longer lists.
const itemTypes = ["article-journal", "book", "chapter", "thesis", "webpage", "report", "dataset"];

const examples = [
  reference("example2026", "Sleep and memory: an example reference", "Example, Sample", "Journal of Synthetic Studies"),
  reference("研究2026", "多言語の文献と記憶の研究", "架空 著者", null, "book"),
  reference("mémoire2026", "La mémoire et les langues du monde", "Auteur Exemple", "Revue des études synthétiques", "thesis"),
];
// The first two share their first author and year; the container tells them apart.
const many = Array.from({ length: 20 }, (_, index) => reference(`example${index + 1}`, `Memory study ${index + 1}: a synthetic reference`,
  "Example, Sample", index % 2 === 0 ? "Journal of Synthetic Studies" : "Proceedings of the Example Conference on Memory and Language",
  itemTypes[index % itemTypes.length]));

const largeSelection = Array.from({ length: 50 }, (_, i) => reference(
  `synthetic_${String(i + 1).padStart(2, "0")}_${"long_key_".repeat(5)}`,
  `Selection study ${i + 1}: a synthetic reference`, "Example, Sample", "Journal of Synthetic Studies"));

export const galleryStates: readonly GalleryState[] = [
  { id: "first-run", label: "First run", library: library({ state: "none", name: null, snapshot_name: null, has_snapshot: false, zotero: false, references: 0 }) },
  { id: "first-run-found", label: "First run, libraries found", library: library({ state: "none", name: null, snapshot_name: null, has_snapshot: false, zotero: false, references: 0 }),
    sources: { zotero: "available", list: 1, files: [{ name: "research.bib", folder: "Documents" }, { name: "reading.bib", folder: "Desktop" }], cut_short: false } },
  { id: "loading", label: "Opening a library", library: library({ state: "loading", has_snapshot: false, zotero: false, snapshot_name: null, references: 0 }) },
  { id: "empty", label: "Empty library", library: library({ name: "empty.bib", snapshot_name: "empty.bib", references: 0 }) },
  { id: "no-results", label: "No matching references", library: library(), query: "unmatched phrase" },
  { id: "many-results", label: "Many results", library: library(), query: "memory", results: many },
  { id: "long-titles", label: "Long and non-Latin titles", library: library(), query: "research", results: [
    reference("long2026", "An intentionally long synthetic title about research across languages, with enough words to wrap across several lines at larger text sizes and in narrow picker windows"),
    examples[1]!, reference("학습2026", "언어와 기억에 관한 가상의 연구", "가상 저자", null, "chapter"),
    reference("स्मृति2026", "भाषाओं और स्मृति पर एक काल्पनिक अध्ययन", "उदाहरण लेखक", null, "report"),
  ] },
  { id: "rtl", label: "Right-to-left text", library: library(), direction: "rtl", query: "الذاكرة", results: [reference("memory2026", "دراسة تخيلية عن الذاكرة واللغات", "كاتب مثال", "Journal of Synthetic Studies", "webpage")] },
  { id: "missing-file", label: "Missing library file", library: library({ state: "error", name: "missing.bib", snapshot_name: null, has_snapshot: false, zotero: false, references: 0, error: "The library file is unavailable." }) },
  { id: "invalid-settings", label: "Invalid settings", library: library({ settings_notice: "settings.json could not be read. Ibis is using the backup. Changes are not being saved. Repair the synthetic settings file and restart Ibis." }) },
  { id: "damaged-library", label: "Damaged library", library: library({ references: 3, warnings: 2, note: "Some references could not be read. Export the library again to restore them." }), query: "memory", results: examples },
  { id: "copy-confirmation", label: "Copy confirmation", library: library(), query: "memory", results: examples, copied: { text: "[@example2026]", warnings: [] } },
  { id: "large-text", label: "Large text", library: library(), query: "memory", results: examples, largeText: true },
  { id: "dark", label: "Dark mode", library: library(), query: "memory", results: examples, theme: "dark" },
  { id: "high-contrast", label: "High contrast", library: library(), query: "memory", results: examples, theme: "contrast" },
  { id: "zotero-updated", label: "Zotero, just updated", library: library({ name: "2 Zotero libraries", snapshot_name: "2 Zotero libraries", zotero: true, snapshot_zotero: true,
    zotero_libraries: ["user", { group: 41 }], references: 1204, updated_at: hoursAgo(2 / 60), checked_at: hoursAgo(2 / 60) }), query: "memory", results: examples },
  { id: "zotero-kept", label: "Zotero closed, not updated", library: library({ state: "error", name: "Zotero library", snapshot_name: "Zotero library", zotero: true, snapshot_zotero: true, stale: true,
    zotero_libraries: ["user"], references: 1204, updated_at: null, checked_at: hoursAgo(2),
    error: "Zotero is not running, or it is not accepting connections; keeping the last loaded references" }), query: "memory", results: examples },
  { id: "copied-notice", label: "Copied notice after hiding", library: library(), query: "memory", results: examples, notice: "[@example2026]" },
  // Browser-only review states (board M); the native evidence app captures the fifteen above.
  { id: "file-kept-menu", label: "Retained file details in More", library: library({ state: "error", name: "example.bib", snapshot_name: "example.bib", stale: true,
    references: 2, error: "The library file is unavailable; keeping the last loaded references" }), query: "memory", results: examples.slice(0, 2), menuOpen: true },
  { id: "pinned", label: "Pinned", library: library(), query: "memory", results: examples, pinned: true },
  { id: "menu-open", label: "More menu open", library: library(), query: "memory", results: examples, menuOpen: true },
  { id: "format-menu", label: "Format menu open", library: library(), query: "memory", results: examples, formatMenu: true },
  { id: "format-menu-filtered", label: "Format menu filtered", library: library(), query: "memory", results: examples, formatMenu: "APA" },
  ...[20, 40, 50].map((count) => ({ id: `tray-${count}`, label: `${count} long keys selected`,
    library: library({ references: 50 }), query: "selection", results: largeSelection, tray: count, pinned: true })),
  { id: "tray-preview", label: "50 selected with formatted preview", library: library({ references: 50 }), query: "selection",
    results: largeSelection, tray: 50, pinned: true, formatted: true },
  { id: "tray-system-large", label: "50 selected at 200% system text", library: library({ references: 50 }), query: "selection",
    results: largeSelection, tray: 50, pinned: true, largeText: true },
  { id: "tray", label: "Several references chosen", library: library(), query: "memory", results: examples, tray: 2 },
  { id: "keys", label: "All keys", library: library(), query: "memory", results: examples, keys: true },
  { id: "compact", label: "Compact rows", library: library(), query: "memory", results: many, density: "compact", tray: 2 },
  { id: "spacious", label: "Spacious rows", library: library(), query: "memory", results: many, density: "spacious", tray: 2 },
  { id: "larger-text", label: "Larger text setting", library: library(), query: "memory", results: examples, largerTextSetting: true },
];

export function galleryProvider(state: GalleryState): { api: PickerApi; preview: PickerPreview } {
  const sources = state.sources;
  return {
    preview: { query: state.query, copied: state.copied },
    api: {
      libraryStatus: async () => state.library,
      ...(sources ? { findSources: async () => sources, openFoundLibrary: async () => null } : {}),
      search: async () => ({ revision: state.library.revision, results: state.results ?? [] }),
      citationText: async (ids) => ({ text: `[${ids.map((id) => `@${id.native_key}`).join("; ")}]`, warnings: [] }),
      copyCitation: async (ids) => ({ text: (state.tray ?? 0) >= 20 ? `[@${ids.map((id) => id.native_key).join("; @")}]` : "[@example2026]", warnings: [] }),
      ...((state.tray ?? 0) >= 20 ? {
        cslItems: async (ids: Parameters<NonNullable<PickerApi["cslItems"]>>[0]) => JSON.stringify(ids.map((id) => ({
          id: id.native_key, type: "book", title: "Synthetic preview pressure", author: [{ family: id.native_key }], issued: { "date-parts": [[2026]] },
        }))),
        copyFormatted: async () => null,
      } : {}),
      copyFormat: async () => "pandoc",
      pickerPreferences: async () => ({ citation_style: state.formatted ? "apa" : null, density: state.density ?? "comfortable", larger_text: state.largerTextSetting ?? false }),
      setPickerPreferences: async () => null,
      shortcutStatus: async () => ({ shortcut: "Ctrl+Shift+Y", registered: true, error: null }),
      chooseLibrary: async () => undefined,
      useZotero: async () => undefined,
      reloadLibrary: async () => undefined,
      hidePicker: async () => undefined,
      setPinned: async () => undefined,
      loginItem: async () => ({ name: "Ibis", enabled: false, problem: null }),
      setLoginItem: async (enabled) => ({ name: "Ibis", enabled, problem: null }),
      quit: async () => undefined,
      onLibraryChanged: async () => () => undefined,
      onPickerShown: async () => () => undefined,
      onPickerHidden: async () => () => undefined,
    },
  };
}
