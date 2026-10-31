import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { App } from "./App";
import { backend, prepared, resetApp, result } from "./testing/app";
import { emit } from "@tauri-apps/api/event";
import type { PickerDestination } from "./api";

// Keep the 2.4-second confirmation alive while assertions run, even when other
// workers are busy. Use one clock from mount through unmount; switching clocks
// with live effects leaves their timers owned by the previous clock.
beforeEach(() => { vi.useFakeTimers(); });
afterEach(resetApp);

async function openPicker(overrides: Parameters<typeof backend>[0] = {}) {
  const calls = backend({ search: () => [result("a", "First")], ...overrides });
  await act(async () => { render(<App />); });
  expect(screen.getByText("First")).toBeTruthy();
  return calls;
}

async function copy() {
  await act(async () => { fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" }); });
}

async function selectPair() {
  const search = screen.getByLabelText("Search references");
  await act(async () => { fireEvent.keyDown(search, { key: "Enter", shiftKey: true }); });
  await act(async () => { fireEvent.keyDown(search, { key: "ArrowDown" }); });
  await act(async () => { fireEvent.keyDown(search, { key: "Enter", shiftKey: true }); });
}

async function toggleKeepOpen() {
  await act(async () => {
    fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "P", code: "KeyP", ctrlKey: true, shiftKey: true });
  });
}

test.each([0, 2])("a permission detour keeps the citation past a minute through %i returns before the grant, waiting for a fresh Enter", async (returnsBeforeGrant) => {
  let granted = false;
  const copies: unknown[] = [];
  const calls = await openPicker({
    search: () => ({ revision: "1", results: [result("a", "First"), result("b", "Second")],
      interpretation: { search: "Smith", locator: { label: "page", value: "12" }, typed: "p.12" } }),
    copy_file_citation: (args) => {
      const citation = prepared(args);
      copies.push(citation.request);
      return { text: citation.preview.text, warnings: [] };
    },
    show_copied_notice: async () => ({ outcome: granted ? "pasted" : "copied", reason: granted ? null : "no-permission", app: "Writer" }),
  });
  const search = screen.getByLabelText("Search references") as HTMLInputElement;
  await act(async () => { fireEvent.change(search, { target: { value: "Smith p.12" } }); });
  await selectPair();
  const shelf = () => document.querySelector(".shelf")?.textContent;
  const shelfText = shelf();
  expect(shelfText).toContain("@a; @b");
  await copy();
  expect(shelf()).toBe(shelfText);
  await act(async () => { await emit("picker:hidden"); await vi.advanceTimersByTimeAsync(61_000); });
  for (let opening = 0; opening < returnsBeforeGrant; opening += 1) {
    await act(async () => { await emit("picker:shown"); });
    expect(shelf()).toBe(shelfText);
    expect(search.value).toBe("Smith p.12");
    await act(async () => { await emit("picker:hidden"); await vi.advanceTimersByTimeAsync(61_000); });
  }
  granted = true;
  await act(async () => { await emit("picker:shown"); });
  expect(shelf()).toBe(shelfText);
  expect(search.value).toBe("Smith p.12");
  expect(calls.filter((c) => c === "copy_file_citation")).toHaveLength(1);
  await act(async () => { fireEvent.keyDown(search, { key: "Enter", repeat: true }); });
  expect(copies).toHaveLength(1);
  await copy();
  expect(calls.filter((c) => c === "copy_file_citation")).toHaveLength(2);
  expect(copies[1]).toEqual(copies[0]);
  expect(copies[1]).toMatchObject({ items: [
    { id: { key: "a" }, locator: { label: "page", value: "12" } },
    { id: { key: "b" }, locator: { label: "page", value: "12" } },
  ] });
  expect(shelf()).not.toContain("@a; @b");
  await act(async () => { await emit("picker:hidden"); await vi.advanceTimersByTimeAsync(61_000); await emit("picker:shown"); });
  expect(search.value).toBe("");
});

test("a preferences event superseding the permission return keeps the prepared output for the next copy", async () => {
  let granted = false;
  let destination: PickerDestination = { app: "Example Editor", app_id: "example.editor", automatic: null, remembered: { key: "typst" }, unreadable: false, can_paste: false, output_question: null };
  let delayReturn = false;
  let finishReturn!: (value: PickerDestination) => void;
  const copies: unknown[] = [];
  const calls = await openPicker({
    picker_destination: () => {
      if (!delayReturn) return destination;
      delayReturn = false;
      return new Promise<PickerDestination>((resolve) => { finishReturn = resolve; });
    },
    copy_file_citation: (args) => {
      const citation = prepared(args);
      copies.push(citation.request);
      return { text: citation.preview.text, warnings: [] };
    },
    show_copied_notice: () => ({ outcome: granted ? "pasted" : "copied", reason: granted ? null : "no-permission", app: "Example Editor" }),
  });
  await copy();
  await act(async () => { await emit("picker:hidden"); });
  destination = { ...destination, app: "Example Writer", app_id: "example.writer", remembered: { key: "cite" } };
  delayReturn = true;
  granted = true;
  await act(async () => { await emit("picker:shown"); });
  expect(calls.filter((call) => call === "picker_destination")).toHaveLength(2);
  await act(async () => { await emit("preferences:changed"); });
  expect(calls.filter((call) => call === "picker_destination")).toHaveLength(3);
  await act(async () => { finishReturn({ ...destination, app: "Stale Editor", app_id: "example.stale" }); });
  expect(screen.getByRole("button", { name: "Copy as Typst key · Example Writer" })).toBeTruthy();
  expect(copies).toHaveLength(1);
  await copy();
  expect(copies).toHaveLength(2);
  expect(copies[1]).toEqual(copies[0]);
  expect(copies[1]).toMatchObject({ format: "typst" });
  expect(screen.getByRole("button", { name: "Copy as \\cite · Example Writer" })).toBeTruthy();
  expect(calls).not.toContain("remember_app_output");
  expect(calls).not.toContain("set_picker_preferences");
});

test.each(["Enter", " "])("copy confirmation expires, but copy warnings remain until dismissed with %s", async (key) => {
  let warnings: string[] = [];
  const calls = await openPicker({ copy_file_citation: () => ({ text: "@a", warnings }) });
  await copy();
  expect(screen.getByText("Copied", { selector: ".shelf-copied, .copy-receipt strong" })).toBeTruthy();
  await act(() => vi.advanceTimersByTimeAsync(2399));
  expect(document.querySelector(".shelf-copied, .copy-notice")).toBeTruthy();
  await act(() => vi.advanceTimersByTimeAsync(1));
  expect(document.querySelector(".shelf-copied, .copy-notice")).toBeNull();

  warnings = ["A warning to read"];
  await copy();
  await act(() => vi.advanceTimersByTimeAsync(3000));
  expect(screen.getByText("A warning to read")).toBeTruthy();
  const dismiss = screen.getByRole("button", { name: "Dismiss copy warnings" });
  const actions = () => calls.filter((command) => command === "copy_file_citation" || command === "hide_picker");
  const previousActions = actions();
  act(() => dismiss.focus());
  const code = key === " " ? "Space" : "Enter";
  fireEvent.keyDown(dismiss, { key, code });
  fireEvent.keyDown(dismiss, { key, code, repeat: true });
  fireEvent.keyUp(dismiss, { key, code });
  expect(screen.queryByLabelText("Copy warnings")).toBeNull();
  expect(actions()).toEqual(previousActions);
});

test("copy remains confirmed when hiding after a successful copy fails", async () => {
  const copies: unknown[] = [];
  const calls = await openPicker({
    search: ({ query }) => query.text === "third" ? [result("c", "Third")] : [result("a", "First"), result("b", "Second")],
    copy_file_citation: (args) => { const citation = prepared(args); copies.push(citation.request); return { text: citation.preview.text, warnings: [] }; },
    hide_picker: () => { throw new Error("SYNTHETIC_PRIVATE_PAYLOAD"); } });
  await selectPair();
  await copy();
  expect(screen.getByText("Ibis could not hide this window. Try again.")).toBeTruthy();
  expect(screen.getByText("Copied", { selector: ".shelf-copied, .copy-receipt strong" })).toBeTruthy();
  expect(document.body.textContent).not.toContain("Nothing was copied");
  expect(document.body.textContent).not.toContain("SYNTHETIC_PRIVATE_PAYLOAD");
  expect(copies[0]).toMatchObject({ items: [{ id: { key: "a" } }, { id: { key: "b" } }] });
  expect(screen.queryByRole("list", { name: "Selected references" })).toBeNull();
  await act(async () => { fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "third" } }); });
  expect(screen.getByText("Third")).toBeTruthy();
  await copy();
  expect(copies).toHaveLength(2);
  expect(copies[1]).toMatchObject({ items: [{ id: { key: "c" } }] });
  expect(calls).not.toContain("show_copied_notice");
});

test("after a pinned batch copy, the live shelf and the next Enter agree", async () => {
  const copies: string[] = [];
  await openPicker({ search: () => [result("a", "First"), result("b", "Second")],
    copy_file_citation: (args) => { const { preview } = prepared(args); copies.push(preview.text); return { text: preview.text, warnings: [] }; } });
  await toggleKeepOpen();
  await selectPair();
  const original = document.querySelector(".shelf-citation")?.textContent;
  await copy();
  expect(copies).toEqual([original]);
  const next = document.querySelector(".shelf-citation")?.textContent;
  expect(next).not.toBe(original);
  await copy();
  expect(copies).toEqual([original, next]);
});

test("editing after a shelf confirmation never announces the old success a second time", async () => {
  await openPicker({ copy_file_citation: (args) => ({ text: prepared(args).preview.text, warnings: [] }) });
  await toggleKeepOpen();
  await copy();
  expect(screen.getByRole("status", { name: "Copied" })).toBeTruthy();
  await act(async () => { fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "newer" } }); });
  expect(screen.queryByRole("status", { name: "Copied" })).toBeNull();
  expect(document.querySelector(".copy-notice p")).toBeNull();
});

test.each(["selection", "search", "reopening"])("a delayed hide failure cannot complete a citation after a newer %s", async (change) => {
  let rejectHide!: (error: Error) => void;
  const calls = await openPicker({
    search: () => [result("a", "First"), result("b", "Second")],
    hide_picker: () => new Promise((_, reject) => { rejectHide = reject; }),
  });
  const search = screen.getByLabelText("Search references");
  await act(async () => { fireEvent.keyDown(search, { key: "Enter", shiftKey: true }); });
  await copy();
  await act(async () => {
    if (change === "selection") fireEvent.click(screen.getByText("Second"), { ctrlKey: true });
    else if (change === "search") fireEvent.change(search, { target: { value: "new search" } });
    else await emit("picker:shown");
  });
  const shelfText = document.querySelector(".shelf")?.textContent;
  await act(async () => { rejectHide(new Error("SYNTHETIC_PRIVATE_PAYLOAD")); });
  expect(document.querySelector(".shelf")?.textContent).toBe(shelfText);
  expect(document.querySelector(".shelf-segment, .shelf-count")).toBeTruthy();
  expect(calls).not.toContain("show_copied_notice");
});

test("a clean copy hides the picker, then says Copied with the copied text", async () => {
  let finishCopy!: () => void;
  let finishHide!: () => void;
  const notices: unknown[] = [];
  const calls = await openPicker({
    copy_file_citation: () => new Promise((resolve) => { finishCopy = () => resolve({ text: "[@a]", warnings: [] }); }),
    hide_picker: () => new Promise<void>((resolve) => { finishHide = resolve; }),
    show_copied_notice: (args) => { notices.push(args); return null; },
  });
  await copy();
  expect(calls).toContain("copy_file_citation");
  expect(calls).not.toContain("hide_picker");
  expect(document.querySelector(".shelf-copied, .copy-notice")).toBeNull();
  await act(async () => { finishCopy(); });
  expect(calls).toContain("hide_picker");
  expect(notices).toEqual([]);
  expect(screen.getByText("Copied", { selector: ".shelf-copied, .copy-receipt strong" })).toBeTruthy();
  await act(async () => { finishHide(); });
  expect(notices).toEqual([{ text: "[@a]", delivery: null, summary: null }]);
  expect(calls.indexOf("hide_picker")).toBeLessThan(calls.indexOf("show_copied_notice"));
});

test("a copy carries its write token through hiding to the notice", async () => {
  let finishHide!: () => void;
  const notices: unknown[] = [];
  const calls = await openPicker({
    copy_file_citation: () => ({ text: "[@a]", warnings: [], delivery: 17 }),
    hide_picker: () => new Promise<void>((resolve) => { finishHide = resolve; }),
    show_copied_notice: (args) => { notices.push(args); return null; },
  });
  await copy();
  expect(calls).toContain("hide_picker");
  expect(notices).toEqual([]);
  await act(async () => { finishHide(); });
  expect(notices).toEqual([{ text: "[@a]", delivery: 17, summary: null }]);
  expect(calls.indexOf("hide_picker")).toBeLessThan(calls.indexOf("show_copied_notice"));
});

test("copies with warnings, pinned copies and failed hides show no Copied notice window", async () => {
  let warnings = ["bare keys cannot carry page numbers"];
  let hideFails = false;
  const calls = await openPicker({
    copy_file_citation: () => ({ text: "a", warnings }),
    hide_picker: () => { if (hideFails) throw new Error("SYNTHETIC"); return null; },
  });
  await copy();
  expect(screen.getByLabelText("Copy warnings")).toBeTruthy();
  await toggleKeepOpen();
  warnings = [];
  await copy();
  expect(calls.filter((c) => c === "copy_file_citation")).toHaveLength(2);
  expect(screen.queryByLabelText("Copy warnings")).toBeNull();
  expect(calls).not.toContain("hide_picker");
  await toggleKeepOpen();
  hideFails = true;
  await copy();
  expect(screen.getByText("Ibis could not hide this window. Try again.")).toBeTruthy();
  expect(calls).not.toContain("show_copied_notice");
});

test("a failed Copied notice never undoes the copy or the hide", async () => {
  const log = vi.spyOn(console, "error").mockImplementation(() => undefined);
  try {
    const calls = await openPicker({
      copy_file_citation: () => ({ text: "@a", warnings: [] }),
      show_copied_notice: () => { throw new Error("SYNTHETIC_PRIVATE_PAYLOAD"); },
    });
    await copy();
    expect(log).toHaveBeenCalledWith("The Copied notice could not be shown.");
    expect(calls).toContain("hide_picker");
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText("Copied", { selector: ".shelf-copied, .copy-receipt strong" })).toBeTruthy();
    expect(log.mock.calls.flat().join(" ")).not.toContain("SYNTHETIC_PRIVATE_PAYLOAD");
  } finally { log.mockRestore(); }
});
