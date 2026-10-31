import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import { closeCitationDetails, backend, prepared, receipt, resetApp, result } from "./testing/app";

beforeEach(() => vi.useFakeTimers());
afterEach(resetApp);
const searchBox = () => screen.getByLabelText("Search references");
const enter = async () => { await act(async () => { fireEvent.keyDown(searchBox(), { key: "Enter" }); }); };
const keys = () => within(screen.getByRole("group", { name: "Keys" }));

test.each(["initial", "new query"])("Enter during %s loading waits for current results and preview, once", async (when) => {
  let finishSearch!: (value: unknown) => void;
  let finishPreview!: (value: ReturnType<typeof receipt>) => void;
  const copied: string[] = [];
  const calls = backend({
    search: ({ query }) => when === "new query" && !query.text ? [result("old", "Old result")]
      : new Promise((resolve) => { finishSearch = resolve; }),
    preview_file_citation: ({ items }) => items[0]!.id.key === "old" ? receipt("@old")
      : new Promise((resolve) => { finishPreview = resolve; }),
    copy_file_citation: (args) => { const { preview } = prepared(args); copied.push(preview.text); return { text: preview.text, warnings: [] }; },
  });
  await act(async () => { render(<App />); });
  if (when === "new query") await act(async () => { fireEvent.change(searchBox(), { target: { value: "new" } }); });
  await enter();
  await enter();
  expect(copied).toEqual([]);
  await act(async () => { finishSearch([result("new", "New result")]); });
  expect(copied).toEqual([]);
  await act(async () => { finishPreview(receipt("@new")); });
  expect(copied).toEqual(["@new"]);
  expect(calls.filter((call) => call === "show_copied_notice")).toHaveLength(1);
});

test("Enter waits during a same-query library retry after a failed search", async () => {
  let finish!: (value: unknown) => void;
  let searches = 0;
  const calls = backend({ search: () => {
    if (++searches === 1) throw new Error("invented search failure");
    return new Promise((resolve) => { finish = resolve; });
  } });
  await act(async () => { render(<App />); });
  expect(screen.getByText("Ibis could not search your library. Use Reload, then try again.")).toBeTruthy();
  await act(async () => { await emit("library:changed", { revision: "2", state: "loaded", has_snapshot: true, references: 1, name: "refs.bib", warnings: 0 }); });
  await enter();
  expect(calls).not.toContain("copy_file_citation");
  await act(async () => { finish([{ ...result("a", "First"), revision: "2" }]); });
  expect(calls.filter((call) => call === "copy_file_citation")).toHaveLength(1);
});

test.each(["query", "selection", "Escape", "hidden", "empty", "failure"])("%s cancels Enter queued during search", async (change) => {
  let finish!: (value: unknown) => void;
  let fail!: (reason: unknown) => void;
  const calls = backend({ search: ({ query }) => query.text === "pending"
    ? new Promise((resolve, reject) => { finish = resolve; fail = reject; }) : [result("a", "First")] });
  await act(async () => { render(<App />); });
  await act(async () => { fireEvent.change(searchBox(), { target: { value: "pending" } }); });
  await enter();
  await act(async () => {
    if (change === "query") fireEvent.change(searchBox(), { target: { value: "different" } });
    if (change === "selection") fireEvent.keyDown(searchBox(), { key: "ArrowDown" });
    if (change === "Escape") fireEvent.keyDown(searchBox(), { key: "Escape" });
    if (change === "hidden") await emit("picker:hidden");
    if (change === "failure") fail(new Error("invented search failure"));
    else finish(change === "empty" ? [] : [result("b", "Second")]);
  });
  expect(calls).not.toContain("copy_file_citation");
});

test.each(["copy", "hide", "delivery"])("double Enter during %s cannot copy or deliver twice", async (stage) => {
  let finish!: () => void;
  const pause = () => new Promise<void>((resolve) => { finish = resolve; });
  const calls = backend({
    search: () => [result("a", "First")],
    copy_file_citation: async () => { if (stage === "copy") await pause(); return { text: "@a", warnings: [] }; },
    hide_picker: async () => { if (stage === "hide") await pause(); },
    show_copied_notice: async () => { if (stage === "delivery") await pause(); return { outcome: "pasted", app: "Example Writer" }; },
  });
  await act(async () => { render(<App />); });
  await enter();
  await enter();
  await act(async () => { fireEvent.click(keys().getByRole("button", { name: /Copy/ })); });
  expect(calls.filter((call) => call === "copy_file_citation")).toHaveLength(1);
  await act(async () => { finish(); });
  expect(calls.filter((call) => call === "show_copied_notice")).toHaveLength(1);
});

test("an empty search keeps the prepared citation, page and Enter available", async () => {
  const copied: unknown[] = [];
  backend({
    search: ({ query }) => query.text ? [] : [result("a", "First")],
    copy_file_citation: (args) => { const citation = prepared(args); copied.push(citation.request); return { text: citation.preview.text, warnings: [] }; },
  });
  await act(async () => { render(<App />); });
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: /^Edit / })); });
  await act(async () => { fireEvent.change(screen.getByRole("textbox", { name: "Page" }), { target: { value: "12" } }); });
  await act(async () => { closeCitationDetails(); });
  await act(async () => { fireEvent.change(searchBox(), { target: { value: "no matches" } }); });
  expect(screen.getByRole("group", { name: "Selected references" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Remove a from the selection" })).toBeTruthy();
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: /^Edit / })); });
  expect((screen.getByRole("textbox", { name: "Page" }) as HTMLInputElement).value).toBe("12");
  await act(async () => { closeCitationDetails(); });
  expect(keys().getByRole("button", { name: /Copy/ })).toBeTruthy();
  await enter();
  expect(copied).toMatchObject([{ items: [{ id: { key: "a" }, locator: null, page_input: "12" }] }]);
});

test("a completed copy cannot be repeated while hidden; reopening requires fresh Enter", async () => {
  const calls = backend({ search: () => [result("a", "First")] });
  await act(async () => { render(<App />); });
  await enter();
  await act(async () => { await emit("picker:hidden"); });
  await enter();
  expect(calls.filter((call) => call === "copy_file_citation")).toHaveLength(1);
  await act(async () => { await emit("picker:shown"); });
  expect(calls.filter((call) => call === "copy_file_citation")).toHaveLength(1);
  await enter();
  expect(calls.filter((call) => call === "copy_file_citation")).toHaveLength(2);
});

test.each([true, false])("the footer names action and output together (paste=%s)", async (canPaste) => {
  backend({
    search: () => [result("a", "First")],
    picker_destination: () => ({ app: "Example Word", app_id: "example.word", automatic: { style: "apa" }, remembered: null, unreadable: false, can_paste: canPaste, output_question: null }),
  });
  await act(async () => { render(<App />); });
  expect(keys().getByRole("button", { name: canPaste ? "Paste into Example Word · APA" : "Copy as APA" })).toBeTruthy();
});

test.each([
  [{ outcome: "pasted", app: "Example Word" }, "Pasted into Example Word"],
  [{ outcome: "copied", reason: "terminal" }, "Copied. Not pasted into a terminal"],
] as const)("More retains the actual delivery outcome after the short feedback expires (%s)", async (delivered, title) => {
  const notices: unknown[] = [];
  backend({ search: () => [result("a", "First")], show_copied_notice: (args) => { notices.push(args); return delivered; } });
  await act(async () => { render(<App />); });
  await enter();
  expect(notices).toEqual([{ text: "@a", delivery: null, summary: null }]);
  await act(async () => { await emit("picker:hidden"); await vi.advanceTimersByTimeAsync(3000); await emit("picker:shown"); });
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: /^More/ })); });
  expect(screen.getByRole("group", { name: new RegExp(`^Last outcome\\. ${title}`) })).toBeTruthy();
}, 10_000);
