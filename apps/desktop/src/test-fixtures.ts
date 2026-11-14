import type { LibraryStatus, SearchResult } from "./api";
import type { RecordId } from "./bindings";

/** Only invented fixture inputs mint wire IDs in frontend code. */
export const recordId = (value: string) => value as unknown as RecordId;

// Invented records shared by component tests. No IPC or real library data.
function library(overrides: Partial<LibraryStatus> = {}): LibraryStatus {
  return { source_generation: "1", source_is_requested: true, revision: "synthetic", state: "loaded", name: "example.bib", snapshot_name: "example.bib", has_snapshot: true, zotero: false, zotero_libraries: [],
    references: 24, source_counts: (overrides.references ?? 24) > 0 ? { "synthetic-test": overrides.references ?? 24 } : {}, warnings: 0, reading: [], live_updates_off: false, error: null, problem: null, note: null, settings_notice: null,
    snapshot_zotero: false, stale: false, updated_at: null, checked_at: hoursAgo(3), ...overrides };
}

/** Times relative to the test run. */
function hoursAgo(hours: number) {
  return Date.now() - hours * 3_600_000;
}

function reference(key: string, title: string, authors = "Example Author", container: string | null = null, itemType = "article-journal"): SearchResult {
  const names = authors.split(", ");
  return { revision: "synthetic", id: { library: "synthetic-test", key: recordId(`SENTINEL-ID-${key}`) }, cite: `synthetic-cite-${key}`, source: "synthetic-test", key, title, authors, year: "2026", matched: ["title"],
    partial: false, first_author: names[0] ?? "", author_count: names.length, container, item_type: itemType };
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

/** Invented engine display nodes for presentation tests. */
export function citationDisplay(spans: readonly { item: number | null; text: string; runs?: readonly (Partial<import("./bindings").DisplayRun> & { text: string })[] }[]): import("./bindings").DisplayTree {
  return { nodes: spans.map((span) => ({ item: span.item, content: (span.runs ?? [{ text: span.text }]).map((run) => ({ italic: false, bold: false, small_caps: false, underline: false, script: "normal", ...run })) })) };
}

/** One typed wire preview for Vitest and the real-browser layout probe. */
export function citationPreview(segments: Parameters<typeof citationDisplay>[0], {
  ticket = "synthetic-ticket", content_hash = ticket, effective_output = null,
  html = null, warnings = [], problem, failure, project_refusal, notices,
}: Partial<Omit<import("./bindings").Preview, "text" | "display">> = {}): import("./bindings").Preview {
  return { ticket, content_hash, effective_output, text: segments.map((segment) => segment.text).join(""),
    html, warnings, display: citationDisplay(segments), problem, failure, project_refusal, notices };
}
