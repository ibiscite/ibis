import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { App } from "./App";
import { backend, chooseFromMore, resetApp, result, findInCitation } from "./testing/app";

// What a person sees of the layer stack: Esc from anywhere, a key and its menu
// item doing the same thing, and nothing surviving a close and reopen.
afterEach(resetApp);

const library = { revision: "1", state: "loaded", has_snapshot: true, zotero: false, snapshot_zotero: false,
  name: "thesis.bib", snapshot_name: "thesis.bib", references: 12, warnings: 0, error: null, problem: null,
  stale: false, zotero_libraries: [], source_counts: { "synthetic-file": 12 }, checked_at: null, updated_at: null };
const search = () => screen.getByLabelText("Search references") as HTMLInputElement;
const press = (key: string, modifiers = {}) => fireEvent.keyDown(document.activeElement ?? document.body, { key, ...modifiers });
const hides = (calls: string[]) => calls.filter((call) => call === "hide_picker").length;
const type = (text: string) => fireEvent.change(search(), { target: { value: text } });

async function chosenCitation(overrides = {}) {
  const calls = backend({ library_status: () => library, search: () => [result("picked", "Invented reference")], ...overrides });
  render(<App />);
  await screen.findByText("Invented reference");
  act(() => search().focus());
  press("Enter", { shiftKey: true });
  const chip = await findInCitation("picked");
  return { calls, chip };
}

test("Esc walks the stack one layer at a time: palette, search, window", async () => {
  const { calls } = await chosenCitation();
  type("invented");
  await chooseFromMore("Keep Window Open");
  const more = screen.getByRole("button", { name: /^More/ });
  act(() => more.focus());
  fireEvent.click(more);
  await screen.findByRole("dialog", { name: "Actions" });
  press("Escape");
  expect(screen.queryByRole("dialog", { name: "Actions" })).toBeNull();
  expect(document.activeElement).toBe(more);
  expect(search().value).toBe("invented");
  press("Escape");
  expect(search().value).toBe("");
  expect(hides(calls)).toBe(0);
  press("Escape");
  await waitFor(() => expect(hides(calls)).toBe(1));
});

