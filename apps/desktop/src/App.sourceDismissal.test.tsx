import { afterEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import { backend, chooseFromMore, resetApp, result } from "./testing/app";

afterEach(resetApp);
const library = { revision: "1", state: "loaded", has_snapshot: true, zotero: false, snapshot_zotero: false,
  name: "thesis.bib", snapshot_name: "thesis.bib", references: 12, warnings: 0, error: null, problem: null,
  stale: false, zotero_libraries: [], source_counts: { "synthetic-file": 12 }, checked_at: null, updated_at: null };
const search = () => screen.getByLabelText("Search references");
const press = (key: string, modifiers = {}) => fireEvent.keyDown(document.activeElement!, { key, ...modifiers });
const commandO = () => press("o", { ctrlKey: true });
const chooseSources = () => chooseFromMore("Choose Library…");
const fileRow = () => screen.getByRole("option", { name: /^Choose a Library File…/ });

test("an accepted file drop leaves Choose Library and shows the dropped library's results", async () => {
  const dropped = { ...library, revision: "2", name: "dropped.bib", snapshot_name: "dropped.bib" };
  let current = library;
  const open = vi.fn(() => null);
  backend({ library_status: () => current,
    open_library_source: open,
    search: () => ({ revision: current.revision, results: [{ ...result("dropped", "Invented dropped reference"), revision: current.revision }] }) });
  render(<App />);
  await screen.findByText("Invented dropped reference");
  act(() => search().focus());
  await chooseSources();
  await screen.findByRole("option", { name: /^Use Zotero/ });
  act(() => fileRow().focus());
  await act(async () => { await emit("library:file-dropped"); });
  current = dropped;
  await act(async () => { await emit("library:changed", dropped); });
  expect(await screen.findByText("Invented dropped reference")).toBeTruthy();
  expect(screen.queryByRole("option", { name: /^Use Zotero/ })).toBeNull();
  expect(screen.getByRole("status", { name: "Library status" }).textContent).toContain("dropped.bib");
  expect(document.activeElement).toBe(search());
  commandO();
  await waitFor(() => expect(open).toHaveBeenCalledOnce());
});
