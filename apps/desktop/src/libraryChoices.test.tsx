import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import { backend, resetApp, result } from "./testing/app";

afterEach(resetApp);
const choiceRow = (name: RegExp) => screen.getByRole("option", { name });
const FILE_ROW = /^BibTeX, BibLaTeX or CSL-JSON file/;
function pressRow(row: HTMLElement) {
  act(() => row.focus());
  fireEvent.keyDown(row, { key: "Enter", code: "Enter" });
  fireEvent.keyUp(row, { key: "Enter", code: "Enter" });
}

test("empty-library choices use arrows and Enter, keep focus after cancelling and hide citation controls", async () => {
  const calls = backend({
    library_status: () => ({ state: "loaded", has_snapshot: true, zotero: false, name: "empty.bib", snapshot_name: "empty.bib", references: 0, warnings: 0, error: null, problem: null, note: null }),
    choose_library: () => null,
  });
  render(<App />);
  const zotero = await screen.findByRole("option", { name: /^Zotero/ });
  const search = screen.getByLabelText("Search references");
  act(() => search.focus());
  fireEvent.keyDown(search, { key: "ArrowDown", code: "ArrowDown" });
  expect(document.activeElement).toBe(zotero);
  fireEvent.keyDown(zotero, { key: "ArrowDown", code: "ArrowDown" });
  const file = choiceRow(FILE_ROW);
  expect(document.activeElement).toBe(file);
  fireEvent.keyDown(file, { key: "ArrowUp", code: "ArrowUp" });
  expect(document.activeElement).toBe(zotero);
  fireEvent.keyDown(zotero, { key: "ArrowDown", code: "ArrowDown" });
  pressRow(file);
  await waitFor(() => expect(calls.filter((call) => call === "choose_library")).toHaveLength(1));
  expect(document.activeElement).toBe(file);
  expect(screen.getAllByRole("option")).toHaveLength(2);
  expect(screen.queryByRole("region", { name: "Preview" })).toBeNull();
  expect(within(document.querySelector("footer")!).queryByRole("button", { name: /^Copy/ })).toBeNull();
  expect(document.querySelector("main")?.getAttribute("data-compact")).toBe("true");
  expect(calls).not.toContain("find_sources");
  expect(calls).not.toContain("copy_file_citation");
});

test("missing-file Find uses the requested name and cancellation leaves its keyboard choices available", async () => {
  const calls = backend({
    library_status: () => ({ state: "error", has_snapshot: false, zotero: false, name: "requested 研究.bib", snapshot_name: null, references: 0, warnings: 0, error: "The library file is unavailable", problem: "missing", note: null }),
    choose_library: () => null,
  });
  render(<App />);
  const find = await screen.findByRole("option", { name: /^Find requested 研究.bib/ });
  pressRow(find);
  await waitFor(() => expect(calls.filter((call) => call === "choose_library")).toHaveLength(1));
  expect(document.activeElement).toBe(find);
  expect(screen.getAllByRole("option")).toHaveLength(2);
  expect(find.getAttribute("data-suggested")).toBe("true");
  expect(screen.queryByRole("region", { name: "Preview" })).toBeNull();
  expect(within(document.querySelector("footer")!).queryByRole("button", { name: /^Copy/ })).toBeNull();
  fireEvent.keyDown(find, { key: "ArrowDown", code: "ArrowDown" });
  const another = choiceRow(/^Use another library/);
  expect(document.activeElement).toBe(another);
  pressRow(another);
  await waitFor(() => expect(document.activeElement).toBe(choiceRow(/^Zotero/)));
});

test("a valid empty replacement hides old citation controls and Enter does nothing", async () => {
  let empty = false;
  const status = () => ({
    revision: empty ? "2" : "1", state: "loaded", has_snapshot: true, zotero: false,
    name: "requested.bib", snapshot_name: "requested.bib", references: empty ? 0 : 1,
    warnings: 0, error: null, problem: null, note: null,
  });
  const calls = backend({
    library_status: status,
    search: () => ({ revision: empty ? "2" : "1", results: empty ? [] : [result("old", "Previously chosen reference")] }),
  });
  render(<App />);
  await screen.findByText("Previously chosen reference");
  const search = screen.getByLabelText("Search references");
  fireEvent.keyDown(search, { key: "Enter", shiftKey: true });
  await screen.findByRole("region", { name: "Preview" });
  empty = true;
  await act(async () => { await emit("library:changed", status()); });
  await screen.findByRole("heading", { name: "No references in requested.bib" });
  expect(document.querySelector(".shelf")).toBeNull();
  expect(within(document.querySelector("footer")!).queryByRole("button", { name: /^Copy/ })).toBeNull();
  expect(document.querySelector("main")?.getAttribute("data-compact")).toBe("true");
  const reads = calls.filter((call) => call === "library_status").length;
  act(() => search.focus());
  fireEvent.keyDown(search, { key: "Enter" });
  fireEvent.keyDown(search, { key: "Enter", shiftKey: true });
  await act(async () => { await Promise.resolve(); });
  expect(calls).not.toContain("copy_file_citation");
  expect(calls.filter((call) => call === "library_status")).toHaveLength(reads);
});

test("an empty first library offers library recovery before asking where you write", async () => {
  let opened = false;
  const status = () => ({
    revision: opened ? "2" : "1", state: opened ? "loaded" : "none", has_snapshot: opened, zotero: false,
    name: opened ? "empty.bib" : null, snapshot_name: opened ? "empty.bib" : null,
    references: 0, warnings: 0, error: null, problem: null, note: null,
  });
  backend({ library_status: status, search: () => ({ revision: opened ? "2" : "1", results: [] }) });
  render(<App />);
  await screen.findByRole("option", { name: FILE_ROW });
  opened = true;
  await act(async () => { await emit("library:changed", status()); });
  await screen.findByRole("heading", { name: "No references in empty.bib" });
  expect(screen.queryByRole("listbox", { name: "What do you write in?" })).toBeNull();
  expect(screen.getAllByRole("option")).toHaveLength(2);
});

test.each(["chip", "preview", "elsewhere"])("an empty replacement returns only removed citation focus to Search (%s)", async (destination) => {
  let empty = false;
  const status = () => ({
    revision: empty ? "2" : "1", state: "loaded", has_snapshot: true, zotero: false,
    name: "requested.bib", snapshot_name: "requested.bib", references: empty ? 0 : 1,
    warnings: 0, error: null, problem: null, note: null,
  });
  backend({
    library_status: status,
    search: () => ({ revision: empty ? "2" : "1", results: empty ? [] : [result("old", "Previously chosen reference")] }),
  });
  render(<App />);
  await screen.findByText("Previously chosen reference");
  const search = screen.getByLabelText("Search references");
  fireEvent.keyDown(search, { key: "Enter", shiftKey: true });
  const preview = await screen.findByRole("region", { name: "Preview" });
  await waitFor(() => expect(preview.tabIndex).toBe(0));
  const announcement = screen.getByRole("status", { name: "Citation selection" });
  expect(announcement.textContent).toBe("1 reference selected for citation");
  const target = destination === "chip" ? screen.getByRole("button", { name: "Remove old from the selection" })
    : destination === "preview" ? preview : screen.getByRole("button", { name: /^More/ });
  act(() => target.focus());
  expect(document.activeElement).toBe(target);

  empty = true;
  await act(async () => { await emit("library:changed", status()); });
  await screen.findByRole("heading", { name: "No references in requested.bib" });
  expect(document.querySelector(".shelf")).toBeNull();
  expect(document.activeElement).toBe(destination === "elsewhere" ? target : search);
  expect(screen.getByRole("status", { name: "Citation selection" })).toBe(announcement);
  expect(announcement.getAttribute("aria-live")).toBe("polite");
  expect(announcement.textContent).toBe("0 references selected for citation");
});

test("an unmatched search and a failed switch retaining a good index keep the chosen citation", async () => {
  const calls = backend({ search: ({ query }) => query.text ? [] : [result("old", "Previously chosen reference")] });
  render(<App />);
  await screen.findByText("Previously chosen reference");
  const search = screen.getByLabelText("Search references");
  fireEvent.keyDown(search, { key: "Enter", shiftKey: true });
  await screen.findByRole("region", { name: "Preview" });
  fireEvent.change(search, { target: { value: "unmatched" } });
  await screen.findByText("No matching references");
  expect(document.querySelector(".shelf")).toBeTruthy();
  expect(screen.queryByRole("option", { name: FILE_ROW })).toBeNull();
  await act(async () => { await emit("library:changed", {
    revision: "1", state: "error", has_snapshot: true, zotero: false,
    name: "missing.bib", snapshot_name: "refs.bib", references: 1,
    warnings: 0, error: "The library file is unavailable", problem: "missing", note: null,
  }); });
  expect(document.querySelector(".shelf")).toBeTruthy();
  expect(within(document.querySelector("footer")!).getByRole("button", { name: /^Copy/ })).toBeTruthy();
  expect(screen.queryByRole("option", { name: /^Find missing.bib/ })).toBeNull();
  expect(calls).not.toContain("copy_file_citation");
});
