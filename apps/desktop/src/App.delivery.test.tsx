import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { App } from "./App";
import { resetApp, result } from "./testing/app";
import { deliveryHarness } from "./testing/delivery";

beforeEach(() => { vi.useFakeTimers(); });
afterEach(resetApp);

type Harness = ReturnType<typeof deliveryHarness>;
const first = result("a", "First invented reference");
const second = result("b", "Second invented reference");
const searchBox = () => screen.getByRole("combobox", { name: "Search references" });

async function open() {
  const harness = deliveryHarness();
  await act(async () => { render(<App />); });
  await act(async () => { harness.search.pending[0]!.complete({ revision: "1", results: [] }); });
  return harness;
}

async function type(query: string) {
  await act(async () => { fireEvent.change(searchBox(), { target: { value: query } }); });
}

async function enter(shiftKey = false, repeat = false) {
  await act(async () => { fireEvent.keyDown(searchBox(), { key: "Enter", shiftKey, repeat }); });
}

async function choose(harness: Harness, query: string, reference = first, text = "[@a]") {
  await type(query);
  await act(async () => { harness.search.pending[0]!.complete({ revision: "1", results: [reference] }); });
  await act(async () => { harness.preview.pending[0]!.complete({ text }); });
}

async function finishCopy(harness: Harness) {
  await act(async () => { harness.copy.pending[0]!.complete(); });
  expect(harness.visible).toBe(false);
}

async function finishDelivery(harness: Harness) {
  await act(async () => { harness.delivery.pending[0]!.complete(); });
  await act(async () => { harness.notice.pending[0]!.complete(); });
}

function expectReceipt(harness: Harness, title: string, text: string) {
  const receipt = within(harness.receipt).getByRole("status");
  expect(receipt.querySelector("strong")?.textContent).toBe(title);
  expect(receipt.querySelector("code")?.textContent).toBe(text);
}

test("Enter waits for a preview, then copies and replaces the document selection exactly once", async () => {
  const harness = await open();
  await type("first");
  await act(async () => { harness.search.pending[0]!.complete({ revision: "1", results: [first] }); });
  await enter();
  await enter(false, true);
  expect(harness.clipboard.owner).toBeNull();
  expect(harness.document.text).toBe("Draft: [citation].");
  await act(async () => { harness.preview.pending[0]!.complete({ text: "[@a]", html: "<i>[@a]</i>" }); });
  expect(harness.copy.pending).toHaveLength(1);
  await finishCopy(harness);
  expect(harness.clipboard).toEqual({ text: "[@a]", html: "<i>[@a]</i>", owner: "ibis", sequence: 1 });
  expect(harness.document.pastes).toEqual([]);
  expect(harness.receipt.textContent).toBe("");
  await finishDelivery(harness);
  expect(harness.document.text).toBe("Draft: [@a].");
  expect(harness.document.pastes).toEqual([{ app: "Example Writer", text: "[@a]", start: 7, end: 17, delivery: 1 }]);
  expectReceipt(harness, "Pasted into Example Writer", "[@a]");
  expect(harness.copy.calls).toHaveLength(1);
  expect(harness.delivery.calls).toHaveLength(1);
});

// Known App.tsx bugs owned by #1401. Remove .fails when integrating its fix:
// an unexpected pass fails the suite, so these cannot silently stay inverted.
test.fails("#1401: Enter during search loading eventually pastes only the new result once", async () => {
  const harness = await open();
  await choose(harness, "old");
  await type("new");
  await enter();
  await enter(false, true);
  expect(harness.copy.calls).toHaveLength(0);
  expect(harness.document.pastes).toEqual([]);
  await act(async () => { harness.search.pending[0]!.complete({ revision: "1", results: [second] }); });
  await act(async () => { harness.preview.pending[0]!.complete({ text: "[@b]" }); });
  // Allow the scenario to reach its document assertion when Enter is dropped.
  if (harness.copy.pending.length > 0) {
    await finishCopy(harness);
    await finishDelivery(harness);
  }
  expect(harness.document.text).toBe("Draft: [@b].");
  expect(harness.document.pastes).toHaveLength(1);
  expectReceipt(harness, "Pasted into Example Writer", "[@b]");
  expect(harness.copy.calls).toHaveLength(1);
  expect(harness.delivery.calls).toHaveLength(1);
});

test.fails("#1401: Shift+Enter cancels a queued Enter even when adding the same reference keeps the preview unchanged", async () => {
  const harness = await open();
  await type("first");
  await act(async () => { harness.search.pending[0]!.complete({ revision: "1", results: [first] }); });
  await enter();
  await enter(true);
  await act(async () => { harness.preview.pending[0]!.complete({ text: "[@a]" }); });
  // Finish an accidentally queued copy to expose the unwanted document edit.
  if (harness.copy.pending.length > 0) {
    await finishCopy(harness);
    await finishDelivery(harness);
  }
  expect(harness.document.text).toBe("Draft: [citation].");
  expect(harness.copy.calls).toHaveLength(0);
  expect(screen.getByRole("button", { name: "Remove a from the selection" })).toBeTruthy();
  expect(harness.clipboard.owner).toBeNull();
  expect(harness.document.text).toBe("Draft: [citation].");
  expect(harness.receipt.textContent).toBe("");
  await enter();
  await finishCopy(harness);
  await finishDelivery(harness);
  expect(harness.document.text).toBe("Draft: [@a].");
  expect(harness.document.pastes).toHaveLength(1);
  expectReceipt(harness, "Pasted into Example Writer", "[@a]");
});

test("late search and preview replies cannot replace the citation that reaches the document", async () => {
  const harness = await open();
  await type("old search");
  const oldSearch = harness.search.pending[0]!;
  await type("old preview");
  await act(async () => { harness.search.pending[1]!.complete({ revision: "1", results: [first] }); });
  const oldPreview = harness.preview.pending[0]!;
  await type("current");
  await act(async () => { harness.search.pending[1]!.complete({ revision: "1", results: [second] }); });
  await enter();
  await act(async () => { harness.preview.pending[1]!.complete({ text: "[@b]" }); });
  await act(async () => {
    oldSearch.complete({ revision: "1", results: [first] });
    oldPreview.complete({ text: "obsolete preview" });
  });
  expect(screen.getByRole("region", { name: "Preview" }).textContent).toContain("[@b]");
  await finishCopy(harness);
  await finishDelivery(harness);
  expect(harness.document.text).toBe("Draft: [@b].");
  expect(harness.document.pastes).toHaveLength(1);
  expectReceipt(harness, "Pasted into Example Writer", "[@b]");
});

test("a copy finishing after a selection edit writes the clipboard but cannot paste or clear the newer citation", async () => {
  const harness = await open();
  await choose(harness, "first");
  await enter();
  await type("second");
  await act(async () => { harness.search.pending[0]!.complete({ revision: "1", results: [second] }); });
  await enter(true);
  await act(async () => { harness.copy.pending[0]!.complete(); });
  expect(harness.clipboard).toMatchObject({ text: "[@a]", owner: "ibis", sequence: 1 });
  expect(harness.document.text).toBe("Draft: [citation].");
  expect(harness.document.pastes).toEqual([]);
  expect(harness.receipt.textContent).toBe("");
  expect(harness.visible).toBe(true);
  expect(screen.getByRole("button", { name: "Remove b from the selection" })).toBeTruthy();
  await act(async () => { harness.preview.pending[0]!.complete({ text: "[@b]" }); });
  await enter();
  await finishCopy(harness);
  await finishDelivery(harness);
  expect(harness.document.text).toBe("Draft: [@b].");
  expect(harness.document.pastes.map(({ delivery }) => delivery)).toEqual([2]);
  expectReceipt(harness, "Pasted into Example Writer", "[@b]");
  expect(harness.copy.calls).toHaveLength(2);
  expect(harness.delivery.calls).toHaveLength(1);
});

test.each(["before paste", "before response"])("reopening %s preserves the new citation and never replays delivery", async (when) => {
  const harness = await open();
  await choose(harness, "first");
  await enter(true);
  await enter();
  await finishCopy(harness);
  if (when === "before response") await act(async () => { harness.delivery.pending[0]!.complete(); });
  await act(async () => { await harness.reopen(); });
  await type("second");
  await act(async () => { harness.search.pending[0]!.complete({ revision: "1", results: [second] }); });
  await enter(true);
  await act(async () => { harness.preview.pending[0]!.complete({ text: "[@a; @b]" }); });
  if (when === "before paste") await act(async () => { harness.delivery.pending[0]!.complete(); });
  await act(async () => { harness.notice.pending[0]!.complete(); });
  expect(screen.getByRole("button", { name: "Remove a from the selection" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Remove b from the selection" })).toBeTruthy();
  expect(harness.visible).toBe(true);
  expect(harness.document.text).toBe(when === "before paste" ? "Draft: [citation]." : "Draft: [@a].");
  expect(harness.document.pastes).toHaveLength(when === "before paste" ? 0 : 1);
  expect(harness.copy.calls).toHaveLength(1);
  expect(harness.delivery.calls).toHaveLength(1);
  expectReceipt(harness, when === "before paste" ? "Copied" : "Pasted into Example Writer", "[@a]");
});

test.each(["different text", "identical text"])("an old notice cannot steal a newer copy's proof (%s)", async (variant) => {
  const harness = await open();
  await choose(harness, "first");
  await enter();
  await finishCopy(harness);
  const oldDelivery = harness.delivery.pending[0]!;
  await act(async () => { await harness.reopen(); });
  const text = variant === "identical text" ? "[@a]" : "[@b]";
  await choose(harness, "second", second, text);
  await enter();
  await finishCopy(harness);
  expect(harness.clipboard.sequence).toBe(2);
  await act(async () => { oldDelivery.complete(); });
  await act(async () => { harness.notice.pending[0]!.complete(); });
  expect(harness.document.text).toBe("Draft: [citation].");
  expect(harness.document.pastes).toEqual([]);
  expectReceipt(harness, "Copied", "[@a]");
  await finishDelivery(harness);
  expect(harness.document.text).toBe(`Draft: ${text}.`);
  expect(harness.document.pastes.map(({ delivery }) => delivery)).toEqual([2]);
  expectReceipt(harness, "Pasted into Example Writer", text);
  expect(harness.copy.calls).toHaveLength(2);
  expect(harness.delivery.calls).toHaveLength(2);
});

test.each(["different text", "identical text"])("another clipboard owner's write prevents a paste (%s)", async (variant) => {
  const harness = await open();
  await choose(harness, "first");
  await enter();
  await finishCopy(harness);
  const text = variant === "identical text" ? "[@a]" : "External invented text";
  harness.copyElsewhere(text);
  await finishDelivery(harness);
  expect(harness.clipboard).toMatchObject({ text, owner: "external", sequence: 2 });
  expect(harness.document.text).toBe("Draft: [citation].");
  expect(harness.document.pastes).toEqual([]);
  expectReceipt(harness, "Copied", "[@a]");
});

test.each(["preview", "copy"] as const)("a failed %s leaves the document and clipboard unchanged and gives no success receipt", async (stage) => {
  const harness = await open();
  await type("first");
  await act(async () => { harness.search.pending[0]!.complete({ revision: "1", results: [first] }); });
  await enter();
  if (stage === "copy") await act(async () => { harness.preview.pending[0]!.complete({ text: "[@a]" }); });
  await act(async () => { harness[stage].pending[0]!.fail({ code: "failed", message: "Invented worker failure" }); });
  expect(screen.getByRole("alert")).toBeTruthy();
  expect(harness.clipboard).toMatchObject({ text: "", owner: null, sequence: 0 });
  expect(harness.document.text).toBe("Draft: [citation].");
  expect(harness.document.pastes).toEqual([]);
  expect(harness.receipt.textContent).toBe("");
  expect(harness.visible).toBe(true);
  expect(harness.delivery.calls).toHaveLength(0);
});
