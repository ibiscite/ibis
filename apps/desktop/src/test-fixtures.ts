import type { LibraryStatus, SearchResult } from "./api";
import type { PickerApi, PickerPreview } from "./App";
import { DEFAULT_PICKER_SHORTCUT } from "./keys";

// Invented records shared by component tests. No IPC or real library data.
function library(overrides: Partial<LibraryStatus> = {}): LibraryStatus {
  return { revision: "synthetic", state: "loaded", name: "example.bib", snapshot_name: "example.bib", has_snapshot: true, zotero: false, zotero_libraries: [],
    references: 24, source_counts: (overrides.references ?? 24) > 0 ? { "synthetic-test": overrides.references ?? 24 } : {}, warnings: 0, error: null, problem: null, note: null, settings_notice: null,
    snapshot_zotero: false, stale: false, updated_at: null, checked_at: hoursAgo(3), ...overrides };
}

/** Times relative to the test run. */
function hoursAgo(hours: number) {
  return Date.now() - hours * 3_600_000;
}

function reference(key: string, title: string, authors = "Example Author", container: string | null = null, itemType = "article-journal"): SearchResult {
  const names = authors.split(", ");
  return { revision: "synthetic", id: { source: "synthetic-test", native_key: key }, key, title, authors, year: 2026, matched: ["title"],
    first_author: names[0] ?? "", author_count: names.length, container, item_type: itemType };
}

// Every type icon appears in the longer lists.
const itemTypes = ["article-journal", "book", "chapter", "thesis", "webpage", "report", "dataset"];

// The first two share their first author and year; the container tells them apart.
const many = Array.from({ length: 20 }, (_, index) => reference(`example${index + 1}`, `Memory study ${index + 1}: a synthetic reference`,
  "Example, Sample", index % 2 === 0 ? "Journal of Synthetic Studies" : "Proceedings of the Example Conference on Memory and Language",
  itemTypes[index % itemTypes.length]));

export const testStates = [
  { id: "no-results", library: library(), query: "unmatched phrase", results: [] },
  { id: "many-results", library: library(), query: "memory", results: many },
  { id: "tray-20", library: library({ references: 50 }), query: "selection", results: [] },
];

export function testProvider(state: typeof testStates[number]): { api: PickerApi; preview: PickerPreview } {
  return {
    preview: { query: state.query },
    api: {
      libraryStatus: async () => state.library,
      search: async () => ({ revision: state.library.revision, results: state.results, interpretation: null }),
      citationText: async (items) => {
        const text = `[${items.map(({ id }) => `@${id.native_key}`).join("; ")}]`;
        return { text, warnings: [], segments: [{ item: null, text }] };
      },
      copyCitation: async () => ({ text: "[@example2026]", warnings: [] }),
      copyFormat: async () => "pandoc",
      pickerPreferences: async () => ({ citation_style: null, density: "comfortable", larger_text: false, show_paste_preview: true, search_mark: false }),
      setPickerPreferences: async () => null,
      shortcutStatus: async () => ({ shortcut: DEFAULT_PICKER_SHORTCUT, registered: true, error: null }),
      chooseLibrary: async () => undefined,
      useZotero: async () => undefined,
      reloadLibrary: async () => undefined,
      hidePicker: async () => undefined,
      pickerVisible: async () => true,
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
