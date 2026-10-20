import type { Copied, LibraryStatus, SearchResult } from "./api";
import type { PickerApi, PickerPreview } from "./App";

// All names and references on this page are synthetic. Providers never call IPC.
type GalleryLibrary = LibraryStatus;
export type GalleryState = {
  id: string;
  label: string;
  library: GalleryLibrary;
  results?: SearchResult[];
  query?: string;
  copied?: Copied;
  theme?: "light" | "dark" | "contrast";
  direction?: "ltr" | "rtl";
  largeText?: boolean;
};

function library(overrides: Partial<GalleryLibrary> = {}): GalleryLibrary {
  return { state: "loaded", name: "example.bib", snapshot_name: "example.bib", has_snapshot: true, zotero: false, zotero_libraries: [],
    references: 24, warnings: 0, error: null, note: null, settings_notice: null, ...overrides };
}

function reference(key: string, title: string, authors = "Example Author"): SearchResult {
  return { id: { source: "synthetic-gallery", native_key: key }, key, title, authors, year: 2026, matched: ["title"] };
}

const examples = [
  reference("example2026", "Sleep and memory: an example reference"),
  reference("研究2026", "多言語の文献と記憶の研究", "架空 著者"),
  reference("mémoire2026", "La mémoire et les langues du monde", "Auteur Exemple"),
];
const many = Array.from({ length: 20 }, (_, index) => reference(`example${index + 1}`, `Memory study ${index + 1}: a synthetic reference`));

export const galleryStates: readonly GalleryState[] = [
  { id: "first-run", label: "First run", library: library({ state: "none", name: null, snapshot_name: null, has_snapshot: false, zotero: false, references: 0 }) },
  { id: "loading", label: "Opening a library", library: library({ state: "loading", has_snapshot: false, zotero: false, snapshot_name: null, references: 0 }) },
  { id: "empty", label: "Empty library", library: library({ name: "empty.bib", snapshot_name: "empty.bib", references: 0 }) },
  { id: "no-results", label: "No matching references", library: library(), query: "unmatched phrase" },
  { id: "many-results", label: "Many results", library: library(), query: "memory", results: many },
  { id: "long-titles", label: "Long and non-Latin titles", library: library(), query: "research", results: [
    reference("long2026", "An intentionally long synthetic title about research across languages, with enough words to wrap across several lines at larger text sizes and in narrow picker windows"),
    examples[1]!, reference("학습2026", "언어와 기억에 관한 가상의 연구", "가상 저자"),
    reference("स्मृति2026", "भाषाओं और स्मृति पर एक काल्पनिक अध्ययन", "उदाहरण लेखक"),
  ] },
  { id: "rtl", label: "Right-to-left text", library: library(), direction: "rtl", query: "الذاكرة", results: [reference("memory2026", "دراسة تخيلية عن الذاكرة واللغات", "كاتب مثال")] },
  { id: "missing-file", label: "Missing library file", library: library({ state: "error", name: "missing.bib", snapshot_name: null, has_snapshot: false, zotero: false, references: 0, error: "The library file is unavailable." }) },
  { id: "invalid-settings", label: "Invalid settings", library: library({ settings_notice: "settings.json could not be read. Ibis is using the backup. Changes are not being saved. Repair the synthetic settings file and restart Ibis." }) },
  { id: "damaged-library", label: "Damaged library", library: library({ references: 3, warnings: 2, note: "Some references could not be read. Export the library again to restore them." }), query: "memory", results: examples },
  { id: "copy-confirmation", label: "Copy confirmation", library: library(), query: "memory", results: examples, copied: { text: "[@example2026]", warnings: [] } },
  { id: "large-text", label: "Large text", library: library(), query: "memory", results: examples, largeText: true },
  { id: "dark", label: "Dark mode", library: library(), query: "memory", results: examples, theme: "dark" },
  { id: "high-contrast", label: "High contrast", library: library(), query: "memory", results: examples, theme: "contrast" },
];

export function galleryProvider(state: GalleryState): { api: PickerApi; preview: PickerPreview } {
  return {
    preview: { query: state.query, copied: state.copied },
    api: {
      libraryStatus: async () => state.library,
      search: async () => state.results ?? [],
      copyCitation: async () => ({ text: "[@example2026]", warnings: [] }),
      copyFormat: async () => "pandoc",
      pickerPreferences: async () => ({ citation_style: null, density: "comfortable", larger_text: false }),
      setPickerPreferences: async () => null,
      shortcutStatus: async () => ({ shortcut: "Ctrl+Shift+Y", registered: true, error: null }),
      chooseLibrary: async () => undefined,
      useZotero: async () => undefined,
      reloadLibrary: async () => undefined,
      hidePicker: async () => undefined,
      setPinned: async () => undefined,
      quit: async () => undefined,
      onLibraryChanged: async () => () => undefined,
      onPickerShown: async () => () => undefined,
      onPickerHidden: async () => () => undefined,
    },
  };
}
