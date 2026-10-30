import { afterEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import { backend, resetApp, result } from "./testing/app";

afterEach(resetApp);
const library = { revision: "1", state: "loaded", has_snapshot: true, zotero: false, snapshot_zotero: false,
  name: "thesis.bib", snapshot_name: "thesis.bib", references: 12, warnings: 0, error: null, problem: null,
  stale: false, zotero_libraries: [], source_counts: { "synthetic-file": 12 }, checked_at: null, updated_at: null };
const search = () => screen.getByLabelText("Search references");
const press = (key: string, modifiers = {}) => fireEvent.keyDown(document.activeElement!, { key, ...modifiers });
const commandO = () => press("o", { ctrlKey: true });
const chooseSources = () => press("o", { ctrlKey: true, shiftKey: true });
const fileRow = () => screen.getByRole("option", { name: /^BibTeX, BibLaTeX or CSL-JSON/ });
const frame = () => act(async () => { await new Promise(requestAnimationFrame); });

test.each([false, true])("source opening leaves Copy as before exposing an accessible failure and recovery (Option %s)", async (altKey) => {
  const atOpen: { menu: HTMLElement | null; focus: Element | null }[] = [];
  const open = vi.fn(() => {
    atOpen.push({ menu: screen.queryByRole("menu"), focus: document.activeElement });
    throw { code: "failed" };
  });
  backend({ library_status: () => library, search: () => [result("picked", "Selected invented reference")], open_library_source: open });
  render(<App />);
  await screen.findByText("Selected invented reference");
  const trigger = within(search().closest("header")!).getByRole("button", { name: /^Copy as / });
  fireEvent.click(trigger);
  await screen.findByRole("searchbox");
  await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("searchbox")));
  press("o", { ctrlKey: true, altKey });
  await waitFor(() => expect(open).toHaveBeenCalledOnce());
  expect(atOpen).toEqual([{ menu: null, focus: search() }]);
  expect(trigger.getAttribute("aria-expanded")).toBe("false");
  const footer = within(document.querySelector("footer")!);
  await waitFor(() => expect(footer.getByRole("alert").textContent).toContain("could not open"));
  expect(footer.getByRole("button", { name: "Find ⁨thesis.bib⁩…" })).toBeTruthy();
});

test("Command-O from Keyboard shortcuts returns focus to Search so the next command invokes recovery", async () => {
  const open = vi.fn(() => { throw { code: "failed" }; });
  backend({ library_status: () => library, open_library_source: open });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("status", { name: "Library status" }).textContent).toContain("thesis.bib"));
  act(() => search().focus());
  press("/", { ctrlKey: true });
  const dialog = await screen.findByRole("dialog", { name: "Keyboard shortcuts" });
  expect(dialog.contains(document.activeElement)).toBe(true);
  commandO();
  await waitFor(() => expect(open).toHaveBeenCalledOnce());
  await screen.findByRole("alert");
  expect(screen.queryByRole("dialog")).toBeNull();
  await frame();
  expect(document.activeElement).toBe(search());
  commandO();
  expect(await screen.findByRole("option", { name: /^Zotero/ })).toBeTruthy();
  expect(open).toHaveBeenCalledOnce();
});

test("Escape from Choose Library restores Search and its source shortcut", async () => {
  const open = vi.fn(() => null);
  backend({ library_status: () => library, open_library_source: open });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("status", { name: "Library status" }).textContent).toContain("thesis.bib"));
  act(() => search().focus());
  chooseSources();
  await screen.findByRole("option", { name: /^Zotero/ });
  act(() => fileRow().focus());
  press("Escape");
  expect(screen.queryByRole("option", { name: /^Zotero/ })).toBeNull();
  expect(document.activeElement).toBe(search());
  commandO();
  await waitFor(() => expect(open).toHaveBeenCalledOnce());
});

test("watcher updates keep Choose Library and its focus until its own selection command completes", async () => {
  let finish!: () => void;
  let current = library;
  const select = vi.fn(() => new Promise<void>((resolve) => { finish = resolve; }));
  backend({ library_status: () => current, search: () => ({ revision: current.revision, results: [] }), use_zotero: select });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("status", { name: "Library status" }).textContent).toContain("thesis.bib"));
  act(() => search().focus());
  chooseSources();
  const zotero = await screen.findByRole("option", { name: /^Zotero/ });
  const file = fileRow();
  act(() => file.focus());
  current = { ...library, revision: "2", references: 14 };
  await act(async () => { await emit("library:changed", current); });
  expect(fileRow()).toBe(file);
  expect(document.activeElement).toBe(file);
  expect(screen.getByRole("status", { name: "Library status" }).textContent).toContain("14 references");
  fireEvent.click(zotero);
  await waitFor(() => expect(select).toHaveBeenCalledOnce());
  current = { ...library, revision: "3", references: 16 };
  await act(async () => { await emit("library:changed", current); });
  expect(fileRow()).toBe(file);
  await act(async () => { finish(); });
  expect(screen.queryByRole("option", { name: /^Zotero/ })).toBeNull();
  expect(document.activeElement).toBe(search());
});

test("a failed source-selection command leaves the chooser available", async () => {
  const select = vi.fn(() => { throw { code: "failed" }; });
  backend({ library_status: () => library, use_zotero: select });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("status", { name: "Library status" }).textContent).toContain("thesis.bib"));
  act(() => search().focus());
  chooseSources();
  const zotero = await screen.findByRole("option", { name: /^Zotero/ });
  fireEvent.click(zotero);
  await waitFor(() => expect(select).toHaveBeenCalledOnce());
  await frame();
  expect(fileRow()).toBeTruthy();
});

test("a selection command from a dismissed chooser cannot dismiss a reopened chooser", async () => {
  let finish!: () => void;
  backend({ library_status: () => library, use_zotero: () => new Promise<void>((resolve) => { finish = resolve; }) });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("status", { name: "Library status" }).textContent).toContain("thesis.bib"));
  act(() => search().focus());
  chooseSources();
  fireEvent.click(await screen.findByRole("option", { name: /^Zotero/ }));
  await waitFor(() => expect(finish).toBeTypeOf("function"));
  press("Escape");
  act(() => search().focus());
  chooseSources();
  const file = await screen.findByRole("option", { name: /^BibTeX, BibLaTeX or CSL-JSON/ });
  act(() => file.focus());
  await act(async () => { finish(); });
  expect(fileRow()).toBe(file);
  expect(document.activeElement).toBe(file);
});
