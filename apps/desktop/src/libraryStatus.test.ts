import { render } from "@testing-library/react";
import { createElement } from "react";
import { expect, test } from "vitest";
import type { LibraryStatus } from "./bindings";
import { libraryCondition, LibraryStatusLine, zoteroLibraryNames } from "./LibraryStatusLine";
import { ago, libraryMessages } from "./libraryWords";

const now = new Date(2026, 9, 20, 12, 0).getTime();
const hour = 3_600_000;
const file = (overrides: Partial<LibraryStatus> = {}): LibraryStatus => ({
  source_generation: "1", source_is_requested: true, revision: "1", state: "loaded", zotero: false, zotero_libraries: [], has_snapshot: true, name: "thesis.bib", snapshot_name: "thesis.bib",
  references: 1204, source_counts: { "synthetic-file": 1204 }, warnings: 0, reading: [], live_updates_off: false, error: null, problem: null, note: null, settings_notice: null, snapshot_zotero: false, stale: false,
  updated_at: null, checked_at: now - 3 * hour, ...overrides,
});
const zotero = (overrides: Partial<LibraryStatus> = {}) => file({
  zotero: true, snapshot_zotero: true, zotero_libraries: ["user"], name: "Zotero library", snapshot_name: "Zotero library", ...overrides,
});
const line = (status: LibraryStatus | null | undefined) =>
  [libraryMessages.summary(status), libraryCondition(status).health === "done" ? null : libraryMessages.age(status, now)].filter(Boolean).join(" · ");
const strip = (text: string | null) => text?.replace(/[⁨⁩]/g, "") ?? null;
const detail = (status: LibraryStatus) =>
  libraryMessages.detail(status, now, (s) => zoteroLibraryNames(s, (id) => `Group ${id}`)).map(strip);
const clockAt = (hours: number) => new Intl.DateTimeFormat("en", { timeStyle: "short" }).format(now - hours * hour);

test("the dot never stays green beside a reported problem, and a worse state stays worse", () => {
  expect(libraryCondition(file()).health).toBe("done");
  expect(libraryCondition(file(), { failure: "Invented failure" }).health).toBe("look");
  // An older copy still searched is a warning; nothing to search is a stop.
  expect(libraryCondition(file({ stale: true }), { groupsFailed: true }).health).toBe("look");
  expect(libraryCondition(file({ state: "error", has_snapshot: false, error: "Invented failure" }), { groupsFailed: true }).health).toBe("stop");
  expect(libraryCondition(null, { groupsFailed: true }).health).toBe("stop");
});

// fixtures/bibtex/damaged.bib, as the engine reads it: a key used twice skips
// both entries under it (decision 0006), two entries cannot be read, and each
// notice comes with the engine's words for why.
const damaged = file({ name: "damaged.bib", snapshot_name: "damaged.bib", references: 3, warnings: 5, reading: [
  { code: "duplicate-key", line: 3, key: "good1" }, { code: "duplicate-key", line: 9, key: "good1" },
  { code: "syntax", line: 21, key: null, why: "it has no citation key" }, { code: "syntax", line: 25, key: null, why: "it never closes" },
  { code: "field", line: 15, key: null, why: "journal uses an abbreviation that is not defined or cannot be expanded" }] });

test("a damaged library says what it skipped, why, and which field to fix", () => {
  expect(line(damaged)).toBe("damaged.bib · 4 entries skipped · 1 to check");
  expect(detail(damaged)).toEqual(["damaged.bib · 3 references · 4 entries skipped · 1 to check",
    `checked 3 hours ago, ${clockAt(3)} · 4 entries skipped: 2 use the key good1 (lines 3 and 9), 2 could not be read (line 21: it has no citation key; line 25: it never closes). Check line 15: journal uses an abbreviation that is not defined or cannot be expanded.`]);
  // More notices than the engine details: the count stays honest.
  expect(line(file({ ...damaged, warnings: 300 }))).toBe("damaged.bib · 300 entries to check");
});

test("relative times are short, never in the future, and turn into dates after a week", () => {
  expect(ago(now + hour, now)).toBe("just now");
  expect(ago(now - 44_000, now)).toBe("just now");
  expect(ago(now - 60_000, now)).toBe("1 minute ago");
  expect(ago(now - 2 * hour, now)).toBe("2 hours ago");
  expect(ago(now - 48 * hour, now)).toBe("2 days ago");
  expect(ago(now - 24 * 8 * hour, now)).toBe("on Oct 12");
});

test("a healthy footer is just the library name; its count and update time stay in detail", () => {
  const { container } = render(createElement(LibraryStatusLine, {
    library: zotero({ updated_at: Date.now(), checked_at: Date.now() }), groupName: String,
  }));
  expect(container.querySelector(".library-status")?.textContent).toBe("Zotero");
  expect(container.querySelector(".status-dot")).toBeNull();
  expect(container.querySelector(".library-age")).toBeNull();
  expect(container.querySelector(".library-detail")?.textContent).toContain("1,204 references · updated just now");
});
