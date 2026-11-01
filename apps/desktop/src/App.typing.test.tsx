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
const footerAction = () => document.querySelector("footer .footer-action[data-variant='primary']");
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
