import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import { backend, prepared, resetApp, result } from "./testing/app";

beforeEach(() => vi.useFakeTimers());
afterEach(resetApp);
const box = () => screen.getByLabelText("Search references");
const enter = async (shiftKey = false) => { await act(async () => { fireEvent.keyDown(box(), { key: "Enter", shiftKey }); }); };
const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((finish) => { resolve = finish; });
  return { promise, resolve };
};

test("queued Enter survives results arriving before their library revision", async () => {
  const found = deferred<unknown>(), status = deferred<unknown>();
  const fresh = { ...result("a", "Fresh reference"), revision: "2" };
  let reads = 0, searches = 0;
  const copies: unknown[] = [];
  backend({
    library_status: () => ++reads === 1 ? { state: "loaded", has_snapshot: true, revision: "1", references: 1 } : status.promise,
    search: () => ++searches === 1 ? found.promise : [fresh],
    copy_file_citation: (args) => { const citation = prepared(args); copies.push(citation.request); return { text: citation.preview.text, warnings: [] }; },
  });
  await act(async () => { render(<App />); });
  await enter();
  await act(async () => { found.resolve([fresh]); });
  expect(reads).toBe(2);
  expect(copies).toEqual([]);
  await act(async () => { status.resolve({ state: "loaded", has_snapshot: true, revision: "2", references: 1 }); });
  expect(copies).toMatchObject([{ expectedRevision: "2", items: [{ id: { key: "a" } }] }]);
});

test("Shift+Enter with no highlighted result cancels the queued copy", async () => {
  const found = deferred<unknown>();
  const calls = backend({ search: () => found.promise });
  await act(async () => { render(<App />); });
  await enter();
  await enter(true);
  await act(async () => { found.resolve([result("a", "First reference")]); });
  expect(calls).not.toContain("copy_file_citation");
  expect(calls).not.toContain("show_copied_notice");
});

test.each(["row", "tile"])("an add activation on a stale %s cancels queued Enter without choosing the stale reference", async (target) => {
  const found = deferred<unknown>();
  const calls = backend({ search: ({ query }) => query.text ? found.promise : [result("old", "Previous reference")] });
  await act(async () => { render(<App />); });
  await act(async () => { fireEvent.change(box(), { target: { value: "loading" } }); });
  await enter();
  await act(async () => {
    if (target === "row") fireEvent.click(screen.getByText("Previous reference"), { shiftKey: true });
    else fireEvent.click(screen.getByTitle(/^Add /));
  });
  await act(async () => { found.resolve([result("a", "Fresh reference")]); });
  expect(screen.queryByRole("button", { name: "Remove old from the selection" })).toBeNull();
  expect(calls).not.toContain("copy_file_citation");
});

test.each(["copy", "hide", "delivery"])("changing only the search cannot repeat the prepared citation during %s", async (stage) => {
  const paused = deferred<void>();
  let copies = 0;
  const calls = backend({
    search: ({ query }) => [result(query.text ? "b" : "a", query.text ? "Another reference" : "Prepared reference")],
    copy_file_citation: async (args) => {
      const { preview } = prepared(args);
      if (++copies === 1 && stage === "copy") await paused.promise;
      return { text: preview.text, warnings: [] };
    },
    hide_picker: async () => { if (stage === "hide") await paused.promise; },
    show_copied_notice: async () => { if (stage === "delivery") await paused.promise; return { outcome: "pasted", app: "Example Writer" }; },
  });
  await act(async () => { render(<App />); });
  await enter(true);
  await enter();
  await act(async () => { fireEvent.change(box(), { target: { value: "another search" } }); });
  expect(screen.getByRole("button", { name: "Remove a from the selection" })).toBeTruthy();
  await enter();
  const copiesWhilePending = copies;
  await act(async () => { paused.resolve(); });
  expect(copiesWhilePending).toBe(1);
  expect(calls.filter((call) => call === "copy_file_citation")).toHaveLength(1);
});

test("changing only chooser scope cannot repeat the prepared citation while hiding", async () => {
  const hidden = deferred<void>();
  const calls = backend({
    search: () => [result("a", "Prepared reference")],
    picker_destination: () => ({ app: "Example Writer", app_id: "example.writer", automatic: null, remembered: null, unreadable: false, can_paste: true }),
    hide_picker: () => hidden.promise,
  });
  await act(async () => { render(<App />); });
  await enter();
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Copy as Pandoc · Example Writer" })); });
  await act(async () => { fireEvent.click(screen.getByRole("menuitemcheckbox", { name: "Just this once" })); });
  await act(async () => { await vi.advanceTimersByTimeAsync(300); });
  await enter();
  const copiesWhileHiding = calls.filter((call) => call === "copy_file_citation").length;
  await act(async () => { hidden.resolve(); });
  expect(copiesWhileHiding).toBe(1);
});

test.each(["failed", "pending", "successful"])("a %s newer attempt preserves ownership of the last successful delivery", async (newer) => {
  const delivered = deferred<unknown>(), secondCopy = deferred<unknown>();
  let copies = 0;
  backend({
    search: () => [result("a", "First reference")],
    copy_file_citation: (args) => {
      if (++copies === 1) return { text: prepared(args).preview.text, warnings: [] };
      if (newer === "failed") throw new Error("invented copy failure");
      if (newer === "pending") return secondCopy.promise;
      return { text: "@a", warnings: ["Invented warning"] };
    },
    show_copied_notice: () => delivered.promise,
  });
  await act(async () => { render(<App />); });
  await enter();
  await act(async () => { await emit("picker:hidden"); await emit("picker:shown"); });
  await enter();
  expect(copies).toBe(2);
  await act(async () => { delivered.resolve({ outcome: "pasted", app: "Example Writer" }); });
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: /^More/ })); });
  const expected = newer === "successful" ? "Copied" : "Pasted into Example Writer";
  const outcome = screen.queryByRole("group", { name: new RegExp(`^Last outcome\\. ${expected}`) });
  if (newer === "pending") {
    await act(async () => { secondCopy.resolve({ text: "@a", warnings: ["Invented warning"] }); });
    expect(screen.getByRole("group", { name: /^Last outcome\. Copied/ })).toBeTruthy();
  }
  expect(outcome).toBeTruthy();
});

test.each(["keyboard", "pointer"])("a different citation can proceed by %s while returning to an outstanding citation cannot repeat it", async (input) => {
  const finishes: ((value: unknown) => void)[] = [];
  backend({ search: () => [result("a", "First reference"), result("b", "Second reference")],
    copy_file_citation: () => new Promise((resolve) => { finishes.push(resolve); }) });
  await act(async () => { render(<App />); });
  await enter();
  if (input === "pointer") await act(async () => { fireEvent.click(screen.getByText("Second reference"), { detail: 2 }); });
  else {
    await act(async () => { fireEvent.keyDown(box(), { key: "ArrowDown" }); });
    await enter();
  }
  await act(async () => { fireEvent.keyDown(box(), { key: "ArrowUp" }); });
  await enter();
  const beforeCompletion = finishes.length;
  await act(async () => { for (const finish of finishes) finish({ text: "synthetic citation", warnings: ["Invented warning"] }); });
  expect(beforeCompletion).toBe(2);
});
