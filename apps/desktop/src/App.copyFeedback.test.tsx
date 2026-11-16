import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { App } from "./App";
import { backend, prepared, resetApp, result, type PreviewRequest, showPicker } from "./testing/app";
import { emit } from "@tauri-apps/api/event";
import type { CitedWarning } from "./bindings";

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

function expectSameCitation(first: PreviewRequest, next: PreviewRequest) {
  const { intent: firstIntent, ...firstCitation } = first;
  const { intent: nextIntent, ...nextCitation } = next;
  expect(nextCitation).toEqual(firstCitation);
  // Returning refreshes status and prepares a new receipt for the same output.
  // Its immutable request identity must advance rather than reuse the old one.
  expect(nextIntent).toEqual({ ...firstIntent, request: expect.any(Number) });
  expect(nextIntent.request).toBeGreaterThan(firstIntent.request);
  expect(nextIntent.cancel).toBe(false);
}

test.each([
  [0, "no-permission"], [2, "no-permission"],
  [0, "accessibility-required"], [2, "accessibility-required"],
] as const)("a permission detour keeps the citation past a minute through %i returns (%s), waiting for a fresh Enter", async (returnsBeforeGrant, reason) => {
  let granted = false;
  const copies: PreviewRequest[] = [];
  const calls = await openPicker({
    search: () => ({ revision: "1", results: [result("a", "First"), result("b", "Second")],
      interpretation: { search: "Smith", locator: { term: "page", value: "12" }, typed: "p.12" } }),
    deliver_citation: (args) => {
      const citation = prepared(args);
      copies.push(citation.request);
      return granted ? { outcome: "pasted", copied: { text: citation.preview.text, warnings: [] }, app: "Writer" } : { outcome: "copied-only", copied: { text: citation.preview.text, warnings: [] }, reason };
    },
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
    await act(async () => { await showPicker(); });
    expect(shelf()).toBe(shelfText);
    expect(search.value).toBe("Smith p.12");
    await act(async () => { await emit("picker:hidden"); await vi.advanceTimersByTimeAsync(61_000); });
  }
  granted = true;
  await act(async () => { await showPicker(); });
  expect(shelf()).toBe(shelfText);
  expect(search.value).toBe("Smith p.12");
  expect(calls.filter((c) => c === "deliver_citation")).toHaveLength(1);
  await act(async () => { fireEvent.keyDown(search, { key: "Enter", repeat: true }); });
  expect(copies).toHaveLength(1);
  await copy();
  expect(calls.filter((c) => c === "deliver_citation")).toHaveLength(2);
  expectSameCitation(copies[0]!, copies[1]!);
  expect(copies[1]).toMatchObject({ items: [
    { id: { key: "a" }, locator: { term: "page", value: "12" } },
    { id: { key: "b" }, locator: { term: "page", value: "12" } },
  ] });
  expect(shelf()).not.toContain("@a; @b");
  await act(async () => { await emit("picker:hidden"); await vi.advanceTimersByTimeAsync(61_000); await showPicker(); });
  expect(search.value).toBe("");
});


const lostPage: CitedWarning = { item: 0, message: { id: "app-loss", args: { what: "the page or location", target: "" } } };

test.each(["Enter", " "])("copy confirmation expires, but copy warnings remain until dismissed with %s", async (key) => {
  let warnings: CitedWarning[] = [];
  const calls = await openPicker({ deliver_citation: () => ({ outcome: "retained", copied: { text: "@a", warnings }, reason: "pinned" }) });
  await copy();
  expect(screen.getByText("Copied", { selector: ".shelf-copied, .copy-notice strong" })).toBeTruthy();
  await act(() => vi.advanceTimersByTimeAsync(2399));
  expect(document.querySelector(".shelf-copied, .copy-notice")).toBeTruthy();
  await act(() => vi.advanceTimersByTimeAsync(1));
  expect(document.querySelector(".shelf-copied, .copy-notice")).toBeNull();

  warnings = [lostPage];
  await copy();
  await act(() => vi.advanceTimersByTimeAsync(3000));
  expect(screen.getByText("This output cannot fully show the page or location.")).toBeTruthy();
  const dismiss = screen.getByRole("button", { name: "Dismiss copy warnings" });
  const actions = () => calls.filter((command) => command === "deliver_citation" || command === "hide_picker");
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
  await openPicker({
    search: ({ query }) => query.text === "third" ? [result("c", "Third")] : [result("a", "First"), result("b", "Second")],
    deliver_citation: (args) => { const citation = prepared(args); copies.push(citation.request); return { outcome: "retained", copied: { text: citation.preview.text, warnings: [] }, reason: "hide-failed" }; },
  });
  await selectPair();
  await copy();
  expect(screen.getByText("Ibis could not hide this window. Try again.")).toBeTruthy();
  expect(screen.getByText("Copied", { selector: ".shelf-copied, .copy-notice strong" })).toBeTruthy();
  expect(document.body.textContent).not.toContain("Nothing was copied");
  expect(document.body.textContent).not.toContain("SYNTHETIC_PRIVATE_PAYLOAD");
  expect(copies[0]).toMatchObject({ items: [{ id: { key: "a" } }, { id: { key: "b" } }] });
  expect(screen.queryByRole("list", { name: "Selected references" })).toBeNull();
  await act(async () => { fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "third" } }); });
  expect(screen.getByText("Third")).toBeTruthy();
  await copy();
  expect(copies).toHaveLength(2);
  expect(copies[1]).toMatchObject({ items: [{ id: { key: "c" } }] });
});

test("after a pinned batch copy, the live shelf and the next Enter agree", async () => {
  const copies: string[] = [];
  await openPicker({ search: () => [result("a", "First"), result("b", "Second")],
    deliver_citation: (args) => { const { preview } = prepared(args); copies.push(preview.text); return { outcome: "retained", copied: { text: preview.text, warnings: [] }, reason: "pinned" }; } });
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
  await openPicker({ deliver_citation: (args) => ({ outcome: "retained", copied: { text: prepared(args).preview.text, warnings: [] }, reason: "pinned" }) });
  await toggleKeepOpen();
  await copy();
  expect(screen.getByRole("status", { name: "Copied" })).toBeTruthy();
  await act(async () => { fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "newer" } }); });
  expect(screen.queryByRole("status", { name: "Copied" })).toBeNull();
  expect(document.querySelector(".copy-notice p")).toBeNull();
});

test.each(["selection", "search", "reopening"])("a delayed hide failure cannot complete a citation after a newer %s", async (change) => {
  let finish!: (outcome: unknown) => void;
  await openPicker({
    search: () => [result("a", "First"), result("b", "Second")],
    deliver_citation: () => new Promise((resolve) => { finish = resolve; }),
  });
  const search = screen.getByLabelText("Search references");
  await act(async () => { fireEvent.keyDown(search, { key: "Enter", shiftKey: true }); });
  await copy();
  await act(async () => {
    if (change === "selection") fireEvent.click(screen.getByText("Second"), { ctrlKey: true });
    else if (change === "search") fireEvent.change(search, { target: { value: "new search" } });
    else await showPicker();
  });
  const shelfText = document.querySelector(".shelf")?.textContent;
  await act(async () => { finish({ outcome: "retained", copied: { text: "@a", warnings: [] }, reason: "hide-failed" }); });
  expect(document.querySelector(".shelf")?.textContent).toBe(shelfText);
  expect(document.querySelector(".citation-token")).toBeTruthy();
});
