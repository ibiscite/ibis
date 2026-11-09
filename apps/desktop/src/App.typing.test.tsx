// Typing keeps the picker still: the screen shows the last answer until the
// next one arrives, and only actions wait for it.
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import { backend, prepared, resetApp, result } from "./testing/app";

beforeEach(() => vi.useFakeTimers());
afterEach(resetApp);
const box = () => screen.getByLabelText("Search references");
const type = async (value: string) => { await act(async () => { fireEvent.change(box(), { target: { value } }); }); };
const footerAction = () => document.querySelector("footer .footer-action[data-variant='action']");
const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((finish) => { resolve = finish; });
  return { promise, resolve };
};
const rows = [result("a", "Gradient methods"), result("b", "Graphs and spectra")];

/** What a person sees: rows, the shelf's citation, the footer action and the empty message. */
const screenState = () => ({
  rows: screen.queryAllByRole("option").map((row) => row.textContent?.includes("Gradient") ? "a" : "b"),
  citation: document.querySelector(".shelf")?.textContent?.includes("@a") ?? false,
  action: footerAction()?.getAttribute("aria-label") ?? null,
  noMatches: screen.queryByText("No matching references") !== null,
});

test("a keystroke leaves the shown results, citation and footer action in place until the reply", async () => {
  const next = deferred<unknown>();
  const calls = backend({ search: ({ query }) => query.text === "gra" ? next.promise : query.text ? rows : [] });
  await act(async () => { render(<App />); });
  await type("gr");
  const settled = screenState();
  expect(settled).toEqual({ rows: ["a", "b"], citation: true, action: "Copy as Pandoc", noMatches: false });
  const reads = { source: calls.filter((c) => c === "library_source").length, preview: calls.filter((c) => c === "preview_file_citation").length };

  await type("gra");
  expect(screenState()).toEqual(settled);
  await act(async () => { next.resolve(rows.slice(0, 1)); });
  expect(screenState()).toEqual({ ...settled, rows: ["a"] });
  // The highlighted reference did not change, so nothing is read again.
  expect(calls.filter((c) => c === "library_source")).toHaveLength(reads.source);
  expect(calls.filter((c) => c === "preview_file_citation")).toHaveLength(reads.preview);
});

test("the first keystroke does not flash No matching references before its reply", async () => {
  const first = deferred<unknown>();
  backend({ search: ({ query }) => query.text ? first.promise : [] });
  await act(async () => { render(<App />); });
  await type("g");
  expect(screenState().noMatches).toBe(false);
  expect(screen.getByText("Type to find a reference")).toBeTruthy();
  await act(async () => { first.resolve([]); });
  expect(screenState().noMatches).toBe(true);
});

test("returning to an answered query announces its count even when a later search was pending", async () => {
  const next = deferred<unknown>();
  const calls = backend({ search: ({ query }) => query.text === "gra" ? next.promise : query.text ? rows.slice(0, 1) : [] });
  await act(async () => { render(<App />); });
  await type("gr");
  await act(() => vi.advanceTimersByTimeAsync(300));
  const status = screen.getByRole("status", { name: "Search results" });
  expect(status.textContent).toBe("1 result");
  await type("gra");
  expect(status.textContent).toBe("");
  const searches = calls.filter((call) => call === "search").length;
  await type("gr");
  expect(calls.filter((call) => call === "search")).toHaveLength(searches);
  expect(screen.getByText("1 result", { selector: "bdi" })).toBeTruthy();
  expect(screen.getByRole("option", { name: /Gradient methods/ })).toBeTruthy();
  await act(async () => { next.resolve(rows); });
  await act(() => vi.advanceTimersByTimeAsync(299));
  expect(status.textContent).toBe("");
  await act(() => vi.advanceTimersByTimeAsync(1));
  expect(status.textContent).toBe("1 result");
  expect(screen.queryByRole("option", { name: /Graphs and spectra/ })).toBeNull();
});

test.each([0, 400])("reopening after %d ms hidden announces the answer it keeps", async (hiddenFor) => {
  const library = { state: "loaded", has_snapshot: true, references: 2 };
  const refresh = deferred<typeof library>();
  let reads = 0;
  const calls = backend({
    search: ({ query }) => query.text ? rows : [],
    library_status: () => reads++ === 0 ? library : refresh.promise,
  });
  await act(async () => { render(<App />); });
  await type("gr");
  await act(() => vi.advanceTimersByTimeAsync(100));
  const status = screen.getByRole("status", { name: "Search results" });
  const searches = calls.filter((call) => call === "search").length;
  await act(async () => { await emit("picker:hidden"); });
  await act(() => vi.advanceTimersByTimeAsync(hiddenFor));
  expect(status.textContent).toBe("");
  await act(async () => { await emit("picker:shown", 2); });
  expect(calls.filter((call) => call === "search")).toHaveLength(searches);
  expect(screen.getAllByRole("option")).toHaveLength(2);
  await act(() => vi.advanceTimersByTimeAsync(200));
  expect(status.textContent).toBe("");
  await act(() => vi.advanceTimersByTimeAsync(100));
  expect(status.textContent).toBe("2 results");
  await act(async () => { refresh.resolve(library); });
});

test("the footer action pressed while a search runs copies the new top result, not the shown one", async () => {
  const next = deferred<unknown>();
  const copies: unknown[] = [];
  backend({
    search: ({ query }) => query.text === "graph" ? next.promise : query.text ? rows : [],
    copy_file_citation: (args) => { const citation = prepared(args); copies.push(citation.request); return { text: citation.preview.text, warnings: [] }; },
  });
  await act(async () => { render(<App />); });
  await type("gr");
  await type("graph");
  await act(async () => { fireEvent.click(footerAction()!); });
  expect(copies).toEqual([]);
  await act(async () => { next.resolve([rows[1]]); });
  expect(copies).toMatchObject([{ items: [{ id: { key: "b" } }] }]);
});

test("a row of the earlier query is still not copied by a click while the search runs", async () => {
  const next = deferred<unknown>();
  const calls = backend({ search: ({ query }) => query.text === "graph" ? next.promise : query.text ? rows : [] });
  await act(async () => { render(<App />); });
  await type("gr");
  await type("graph");
  await act(async () => { fireEvent.click(screen.getByText("Gradient methods"), { detail: 2 }); });
  await act(async () => { next.resolve([]); });
  expect(calls).not.toContain("copy_file_citation");
});

test("rows of an older library snapshot are not formatted or offered while the new search runs", async () => {
  const next = deferred<unknown>();
  let revision = "1";
  const calls = backend({
    library_status: () => ({ state: "loaded", has_snapshot: true, revision, references: 2 }),
    search: ({ query }) => revision === "2" && query.text ? next.promise : query.text ? rows : [],
  });
  await act(async () => { render(<App />); });
  await type("gr");
  const previews = calls.filter((c) => c === "preview_file_citation").length;
  revision = "2";
  await act(async () => { await emit("library:changed", { state: "loaded", has_snapshot: true, revision: "2", references: 2 }); });
  expect(calls.filter((c) => c === "preview_file_citation")).toHaveLength(previews);
  expect(screen.queryByText(/The library changed/)).toBeNull();
  expect(footerAction()).toBeNull();
  await act(async () => { next.resolve(rows.map((row) => ({ ...row, revision: "2" }))); });
  expect(screenState().action).toBe("Copy as Pandoc");
});
