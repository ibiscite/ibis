import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "./App";
import { backend, resetApp, result } from "./testing/app";

afterEach(resetApp);

test.each(["welcome", "empty", "loaded"])("%s: the native smoke's More and Quit route accepts virtual activation", async (state) => {
  const calls = backend({
    library_status: () => ({ revision: "1", state: state === "welcome" ? "none" : "loaded", has_snapshot: state !== "welcome",
      zotero: false, snapshot_zotero: false, zotero_libraries: [], name: state === "welcome" ? null : "refs.bib",
      snapshot_name: state === "welcome" ? null : "refs.bib", references: state === "loaded" ? 3 : 0,
      source_counts: {}, warnings: 0, error: null, problem: null, note: null, settings_notice: null,
      stale: false, checked_at: null, updated_at: null }),
    quit: () => null,
  });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("status", { name: "Library status" }).textContent)
    .toBe(state === "welcome" ? "No library chosen" : `refs.bib · ${state === "empty" ? "no references" : "3 references"}`));
  const button = screen.getByRole<HTMLButtonElement>("button", { name: "More" });
  expect(button.tagName).toBe("BUTTON");
  expect(button.type).toBe("button");
  expect(button.disabled).toBe(false);
  expect(button.tabIndex).toBe(0);
  expect(button.getAttribute("aria-haspopup")).toBe("dialog");
  act(() => button.click());
  const palette = await screen.findByRole("dialog", { name: "Actions" });
  act(() => within(palette).getByRole("option", { name: "Quit Ibis" }).click());
  await waitFor(() => expect(calls.filter((command) => command === "quit")).toHaveLength(1));
});

test("with a result ready to copy, the citation format is a distinct named control and can be changed by key", async () => {
  const saved: unknown[] = [];
  const calls = backend({
    search: ({ query }) => query.text ? [result("invented", "Invented reference")] : [],
    picker_preferences_saved: ({ preferences }) => { saved.push((preferences as { output?: string }).output ?? null); return null; },
  });
  render(<App />);
  const search = screen.getByLabelText("Search references");
  fireEvent.change(search, { target: { value: "invented" } });
  const copy = await within(screen.getByRole("group", { name: "Keys" })).findByRole("button", { name: "Copy as Pandoc" });
  expect(screen.getAllByRole("button", { name: "Copy as Pandoc" })).toEqual([copy]);
  const button = screen.getByRole<HTMLButtonElement>("button", { name: "Citation format Pandoc" });
  const status = screen.getByRole("status", { name: "Citation format" });
  expect(status.textContent).toMatch(/^Pandoc\./);
  expect(button.tagName).toBe("BUTTON");
  expect(button.type).toBe("button");
  expect(button.disabled).toBe(false);
  expect(button.tabIndex).toBe(0);
  expect(typeof button.onclick).toBe("function");
  act(() => button.click());
  const filter = await screen.findByRole("combobox", { name: /^Search (\d+ formats|actions)$/ });
  fireEvent.change(filter, { target: { value: "Typst" } });
  await screen.findByRole("option", { name: "Typst" });
  fireEvent.keyDown(filter, { key: "ArrowDown" });
  const choice = document.activeElement!;
  fireEvent.keyDown(choice, { key: "Enter" });
  fireEvent.keyUp(choice, { key: "Enter" });
  await waitFor(() => expect(saved).toEqual(["typst"]));
  expect(await screen.findByRole("button", { name: "Citation format Typst" })).toBe(button);
  expect(status.textContent).toMatch(/^Typst\./);
  expect(document.activeElement).toBe(search);
  expect(calls).not.toContain("copy_file_citation");
});

test("More holds working actions, leaving preferences and update checks to Settings", async () => {
  const calls = backend({ login_item: () => ({ name: "Ibis", enabled: true, problem: null }),
    update_checks: () => true, update_available: () => true, check_for_updates: () => null });
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: /^More/ }));
  const palette = await screen.findByRole("dialog", { name: "Actions" });
  const labels = within(palette).getAllByRole("option").map((row) => row.querySelector(".ui-palette-label")?.textContent);
  expect(labels).toEqual(expect.arrayContaining(["Open ⁨refs.bib⁩", "Keep Window Open", "Keyboard Shortcuts", "Settings…", "Quit Ibis"]));
  // Preferences (Rows, login, updates) live in Settings; More only acts.
  expect(labels.filter((label) => /Rows|Login|Updates/.test(label ?? ""))).toEqual([]);
  expect(calls).not.toContain("picker_preferences_saved");
  expect(calls).not.toContain("forget_app_outputs");
});

