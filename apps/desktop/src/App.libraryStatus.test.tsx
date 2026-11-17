import { afterEach, expect, test, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "./App";
import type { LibraryStatus } from "./api";
import { backend, resetApp, result } from "./testing/app";

afterEach(resetApp);
const hour = 3_600_000;
const library = (overrides: Partial<LibraryStatus> = {}): LibraryStatus => ({
  source_generation: "1", source_is_requested: true, revision: "1", state: "loaded", has_snapshot: true, zotero: true, snapshot_zotero: true,
  zotero_libraries: ["user"], name: "Zotero library", snapshot_name: "Zotero library",
  references: 12, source_counts: { "zotero:user": 12 }, warnings: 0, reading: [], live_updates_off: false, error: null, problem: null,
  note: null, settings_notice: null, stale: false, checked_at: Date.now() - 2 * hour, updated_at: null, ...overrides,
});
const stale = (overrides: Partial<LibraryStatus> = {}) => library({ state: "error", stale: true,
  error: "The library could not be refreshed; keeping the last loaded references.", problem: "zotero_closed", ...overrides });
const status = () => screen.getByRole("status", { name: "Library status" });
const footer = () => document.querySelector("footer")!;

test("an unknown file is a failed open with a chooser listing RIS and CSV", async () => {
  const reason = "This file is not in a format Ibis reads: BibTeX, BibLaTeX, CSL-JSON, RIS or CSV.";
  const choose = vi.fn(() => null);
  backend({ library_status: () => library({ state: "error", has_snapshot: false, zotero: false, snapshot_zotero: false,
    name: "notes.xyz", snapshot_name: null, references: 0, problem: "unreadable", error: reason }), choose_library: choose });
  render(<App />);
  expect(await screen.findByRole("heading", { name: /Can’t read notes.xyz/ })).toBeTruthy();
  expect(document.body.textContent).toContain(reason);
  expect(document.body.textContent).not.toContain("No references in");
  fireEvent.click(screen.getByRole("option", { name: /^Use Another Library/ }));
  const file = await screen.findByRole("option", { name: /^Choose a Library File/ });
  expect(file.textContent).toContain("BibTeX, BibLaTeX, CSL-JSON, RIS or CSV");
  fireEvent.click(file);
  await waitFor(() => expect(choose).toHaveBeenCalledOnce());
  expect(document.body.textContent).not.toContain("No references in");
});

test("a moved file keeps its usable references and offers Find once in the footer", async () => {
  const choose = vi.fn(() => null);
  backend({ library_status: () => stale({ zotero: false, snapshot_zotero: false, name: "thesis.bib", snapshot_name: "thesis.bib", problem: "missing" }),
    search: () => [result("a", "Retained reference")], choose_library: choose });
  render(<App />);
  await screen.findByText("Retained reference");
  expect(status().textContent).toBe("thesis.bib moved");
  expect(document.querySelector(".ui-line-notice, .notice-slot")).toBeNull();
  fireEvent.click(within(footer()).getByRole("button", { name: "Find ⁨thesis.bib⁩…" }));
  await waitFor(() => expect(choose).toHaveBeenCalledOnce());
});
