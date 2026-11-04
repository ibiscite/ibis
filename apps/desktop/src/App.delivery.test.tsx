import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { App } from "./App";
import type { Copied, Delivered, FilePreview } from "./bindings";
import { resetApp, result } from "./testing/app";
import { deliveryHarness } from "./testing/delivery";

beforeEach(() => { vi.useFakeTimers(); });
afterEach(resetApp);

type Harness = ReturnType<typeof deliveryHarness>;
const first = result("a", "First invented reference");
const second = result("b", "Second invented reference");
const searchBox = () => screen.getByRole("combobox", { name: "Search references" });

async function open(configure?: (harness: Harness) => void) {
  const harness = deliveryHarness();
  configure?.(harness);
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

async function finishSearch(harness: Harness, query: string, results: typeof first[]) {
  const jobs = harness.search.pending.filter((job) => job.args.text === query);
  expect(jobs).toHaveLength(1);
  await act(async () => { jobs[0]!.complete({ revision: "1", results }); });
}

async function choose(harness: Harness, query: string, reference = first, text = "[@a]") {
  await type(query);
  await finishSearch(harness, query, [reference]);
  const preview = harness.preview.pending.at(-1)!;
  expect(preview.args.items.map(({ id }) => id)).toEqual([reference.id]);
  await act(async () => { preview.complete({ text }); });
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

test.each([
  ["unknown app", (h: Harness) => { h.target.verified = false; }, "Copied. Automatic paste is not approved for this app. Press ⌘V to paste."],
  ["sleep and wake", (h: Harness) => { h.lifecycle.epoch += 2; }, "Copied. The paste expired. Press ⌘V to paste."],
  ["lock and unlock", (h: Harness) => { h.lifecycle.epoch += 2; }, "Copied. The paste expired. Press ⌘V to paste."],
  ["long delay", (h: Harness) => { h.lifecycle.elapsed += 1500; }, "Copied. The paste expired. Press ⌘V to paste."],
  ["safety check unavailable", (h: Harness) => { h.lifecycle.available = false; }, "Copied. Press ⌘V to paste."],
  ["AX unavailable", (h: Harness) => { h.target.focusAvailable = false; }, "Copied. Press ⌘V to paste."],
  ["AX timeout", (h: Harness) => { h.target.focusTimedOut = true; }, "Copied. Press ⌘V to paste."],
  ["foreign nonactivating panel holds keyboard focus", (h: Harness) => { h.target.focusedPid = 99; }, "Copied. The target app or window changed. Press ⌘V to paste."],
  ["clipboard changed", (h: Harness) => { h.clipboard.sequence += 1; }, "Copied. The clipboard changed since. Copy the citation again."],
  ["another window", (h: Harness) => { h.target.window = 200; }, "Copied. The target app or window changed. Press ⌘V to paste."],
  ["closed window", (h: Harness) => { h.target.window = null; }, "Copied. The target app or window changed. Press ⌘V to paste."],
  ["minimised window", (h: Harness) => { h.target.available = false; }, "Copied. The target app or window changed. Press ⌘V to paste."],
  ["quit app", (h: Harness) => { h.target.available = false; }, "Copied. The target app or window changed. Press ⌘V to paste."],
] as const)("%s refuses pending delivery, keeps the copied citation and shows its exact reason", async (_scenario, change, message) => {
  const harness = await open();
  await choose(harness, "first");
  await enter();
  await finishCopy(harness);
  change(harness);
  await finishDelivery(harness);
  expect(harness.clipboard.text).toBe("[@a]");
  expect(harness.document.text).toBe("Draft: [citation].");
  expect(harness.document.pastes).toEqual([]);
  expectReceipt(harness, message, "[@a]");
});

test("unproven macOS focus copies with an unavailable reason", async () => {
  const harness = await open((harness) => { harness.target.window = null; });
  // Missing identity at capture is unavailable, not a changed target.
  await choose(harness, "first");
  await enter();
  await finishCopy(harness);
  await finishDelivery(harness);
  expectReceipt(harness, "Copied. Press ⌘V to paste.", "[@a]");
  expect(harness.clipboard.text).toBe("[@a]");
  expect(harness.document.pastes).toEqual([]);
});

test("a foreign panel present at capture is unavailable evidence, not a changed captured target", async () => {
  const harness = await open((harness) => { harness.target.focusedPid = 99; });
  await choose(harness, "first");
  await enter();
  await finishCopy(harness);
  await finishDelivery(harness);
  expect(harness.document.pastes).toEqual([]);
  expect(harness.clipboard.text).toBe("[@a]");
  expectReceipt(harness, "Copied. Press ⌘V to paste.", "[@a]");
});

test("opening an attached Save sheet keeps the document identity but refuses paste into the new surface", async () => {
  const harness = await open();
  const sheet = { text: "invented-name", selection: [0, 13] as [number, number] };
  harness.documents.set(500, sheet);
  await choose(harness, "first");
  await enter();
  await finishCopy(harness);
  harness.target.surface = 500;
  expect(harness.target.window).toBe(100);
  await finishDelivery(harness);
  expect(harness.document.text).toBe("Draft: [citation].");
  expect(sheet.text).toBe("invented-name");
  expect(harness.document.pastes).toEqual([]);
  expect(harness.clipboard.text).toBe("[@a]");
  expectReceipt(harness, "Copied. The target app or window changed. Press ⌘V to paste.", "[@a]");
});

test.each([false, true])("missing Accessibility with PostEvent granted keeps its own receipt, including a later grant (%s)", async (grantBeforeDelivery) => {
  const harness = await open((harness) => { harness.authorizations.accessibility = false; });
  await choose(harness, "first");
  await enter();
  await finishCopy(harness);
  harness.authorizations.accessibility = grantBeforeDelivery;
  await finishDelivery(harness);
  expect(harness.document.text).toBe("Draft: [citation].");
  expect(harness.document.pastes).toEqual([]);
  expect(harness.clipboard.text).toBe("[@a]");
  expectReceipt(harness, "Copied. Ibis needs Accessibility access to paste safely. Press ⌘V to paste.", "[@a]");
  harness.authorizations.accessibility = true;
  await act(async () => { window.dispatchEvent(new Event("focus")); await vi.advanceTimersByTimeAsync(2000); });
  expect(harness.delivery.calls).toHaveLength(1);
  expect(harness.document.pastes).toEqual([]);
});

test("granting Accessibility needs a fresh external summon and Enter before pasting", async () => {
  const harness = await open((harness) => { harness.authorizations.accessibility = false; });
  await choose(harness, "first");
  await enter();
  await finishCopy(harness);
  await finishDelivery(harness);
  expectReceipt(harness, "Copied. Ibis needs Accessibility access to paste safely. Press ⌘V to paste.", "[@a]");
  harness.authorizations.accessibility = true;
  expect(harness.document.pastes).toEqual([]);
  await act(async () => { await harness.reopen(); });
  await choose(harness, "second", second, "[@b]");
  expect(harness.document.pastes).toEqual([]);
  await enter();
  await finishCopy(harness);
  await finishDelivery(harness);
  expect(harness.document.text).toBe("Draft: [@b].");
  expect(harness.document.pastes).toHaveLength(1);
  expectReceipt(harness, "Pasted into Example Writer", "[@b]");
});

test.each([100, 200])("Enter pastes into focused window %s with two document windows open", async (window) => {
  const harness = await open((harness) => { harness.target.window = window; });
  await choose(harness, "first");
  await enter();
  await finishCopy(harness);
  await finishDelivery(harness);
  expect(harness.documents.get(window)?.text).toBe(window === 100 ? "Draft: [@a]." : "Second: [@a].");
  expect(harness.documents.get(window === 100 ? 200 : 100)?.text).toBe(window === 100 ? "Second: [citation]." : "Draft: [citation].");
  expect(harness.document.pastes).toHaveLength(1);
  expect(harness.clipboard.text).toBe("[@a]");
  expectReceipt(harness, "Pasted into Example Writer", "[@a]");
});

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

test("Enter during search loading eventually pastes only the new result once", async () => {
  const harness = await open();
  await choose(harness, "old");
  await type("new");
  await enter();
  await enter(false, true);
  expect(harness.copy.calls).toHaveLength(0);
  expect(harness.document.pastes).toEqual([]);
  await act(async () => { harness.search.pending[0]!.complete({ revision: "1", results: [second] }); });
  await act(async () => { harness.preview.pending[0]!.complete({ text: "[@b]" }); });
  expect(harness.copy.pending).toHaveLength(1);
  await finishCopy(harness);
  await finishDelivery(harness);
  expect(harness.document.text).toBe("Draft: [@b].");
  expect(harness.document.pastes).toHaveLength(1);
  expectReceipt(harness, "Pasted into Example Writer", "[@b]");
  expect(harness.copy.calls).toHaveLength(1);
  expect(harness.delivery.calls).toHaveLength(1);
});

test("Shift+Enter cancels a queued Enter even when adding the same reference keeps the preview unchanged", async () => {
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
  expect(screen.getByRole("status", { name: "Citation selection" }).textContent).toBe("1 reference selected for citation");
  await act(async () => { harness.preview.pending[0]!.complete({ text: "[@b]" }); });
  expect(screen.getByRole("button", { name: "Remove b from the selection" })).toBeTruthy();
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
  const recheck = harness.search.pending.at(-1)!;
  expect(recheck.args.text).toBe("first");
  await type("second");
  await finishSearch(harness, "second", [second]);
  await enter(true);
  const preview = harness.preview.pending.at(-1)!;
  expect(preview.args.items.map(({ id }) => id)).toEqual([first.id, second.id]);
  await act(async () => { preview.complete({ text: "[@a; @b]" }); });
  // The status read on reopening also searched the previous query. Complete
  // that worker late: it must not remove the newer selection or start a copy.
  await act(async () => { recheck.complete({ revision: "1", results: [first] }); });
  if (when === "before paste") await act(async () => { harness.delivery.pending[0]!.complete(); });
  await act(async () => { harness.notice.pending[0]!.complete(); });
  expect(screen.getByRole("button", { name: "Remove a from the selection" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Remove b from the selection" })).toBeTruthy();
  expect(harness.visible).toBe(true);
  expect(harness.document.text).toBe(when === "before paste" ? "Draft: [citation]." : "Draft: [@a].");
  expect(harness.document.pastes).toHaveLength(when === "before paste" ? 0 : 1);
  expect(harness.copy.calls).toHaveLength(1);
  expect(harness.delivery.calls).toHaveLength(1);
  expectReceipt(harness, when === "before paste" ? "Copied. Press ⌘V to paste." : "Pasted into Example Writer", "[@a]");
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
  expect(harness.receipt.textContent).toBe("");
  await finishDelivery(harness);
  expect(harness.document.text).toBe(`Draft: ${text}.`);
  expect(harness.document.pastes.map(({ delivery }) => delivery)).toEqual([2]);
  expectReceipt(harness, "Pasted into Example Writer", text);
  expect(harness.copy.calls).toHaveLength(2);
  expect(harness.delivery.calls).toHaveLength(2);
});

test.each(["[@b]", "[@a]"])("a refused A finishing after B's receipt cannot replace B's delivery message (%s)", async (text) => {
  const harness = await open();
  await choose(harness, "first");
  await enter();
  await finishCopy(harness);
  const a = harness.delivery.pending[0]!;
  await act(async () => { await harness.reopen(); });
  await choose(harness, "second", second, text);
  await enter();
  await finishCopy(harness);
  await act(async () => { harness.delivery.pending[1]!.complete(); });
  await act(async () => { harness.notice.pending[0]!.complete(); });
  expectReceipt(harness, "Pasted into Example Writer", text);
  await act(async () => { a.complete(); });
  await act(async () => { harness.notice.pending[0]!.complete(); });
  expectReceipt(harness, "Pasted into Example Writer", text);
  expect(harness.clipboard.text).toBe(text);
  expect(harness.document.pastes.map(({ delivery }) => delivery)).toEqual([2]);
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
  expectReceipt(harness, "Copied. The clipboard changed since. Copy the citation again.", "[@a]");
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


test("the fake captures a copy's app and presentation at its successful write", async () => {
  const harness = deliveryHarness();
  const pendingPreview = invoke<FilePreview>("preview_file_citation", {
    items: [], format: "pandoc", style: null, expectedRevision: "1",
    intent: { page: 1, request: 1, cancel: false },
  });
  await act(async () => { harness.preview.pending[0]!.complete({ text: "[@a]" }); });
  const preview = await pendingPreview;
  const pendingCopy = invoke<Copied>("copy_file_citation", { ticket: preview.ticket, contentHash: preview.content_hash });
  harness.document.app = "Reopened Writer";
  await act(async () => { await harness.reopen(); });
  await act(async () => { harness.copy.pending[0]!.complete(); });
  const copied = await pendingCopy;
  // Bypass App's stale-completion guard to prove this native write can paste.
  // A fake that freezes the old presentation would mask that UI regression.
  await act(async () => { await invoke("hide_picker"); });
  const pendingNotice = invoke<Delivered>("show_copied_notice", { text: copied.text, delivery: copied.delivery });
  await finishDelivery(harness);
  expect(await pendingNotice).toEqual({ outcome: "pasted", app: "Reopened Writer" });
  expect(harness.document.text).toBe("Draft: [@a].");
  expect(harness.document.pastes).toEqual([{ app: "Reopened Writer", text: "[@a]", start: 7, end: 17, delivery: 1 }]);
  expectReceipt(harness, "Pasted into Reopened Writer", "[@a]");
});

test("reopening during a pending copy keeps the new query and selection without delivering the late copy", async () => {
  const harness = await open();
  await choose(harness, "first");
  await enter(true);
  await enter();
  expect(harness.copy.pending).toHaveLength(1);
  await act(async () => { await harness.reopen(); });
  await type("second query");
  await finishSearch(harness, "second query", [second]);
  await enter(true);
  await act(async () => { harness.copy.pending[0]!.complete(); });
  expect(harness.clipboard).toMatchObject({ text: "[@a]", owner: "ibis", sequence: 1 });
  expect(harness.visible).toBe(true);
  expect((searchBox() as HTMLInputElement).value).toBe("second query");
  expect(screen.getByRole("status", { name: "Citation selection" }).textContent).toBe("2 references selected for citation");
  // Reopening can leave the old single-reference preview queued. Complete the
  // request for the current selection, independently of worker arrival order.
  const previews = harness.preview.pending.filter(({ args }) =>
    args.items.length === 2 && args.items[0]!.id.key === "a" && args.items[1]!.id.key === "b");
  expect(previews).toHaveLength(1);
  await act(async () => { previews[0]!.complete({ text: "[@a; @b]" }); });
  expect(screen.getByRole("button", { name: "Remove a from the selection" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Remove b from the selection" })).toBeTruthy();
  expect(harness.delivery.calls).toHaveLength(0);
  expect(harness.document.text).toBe("Draft: [citation].");
  expect(harness.document.pastes).toEqual([]);
  expect(harness.receipt.textContent).toBe("");
});

const finishOpening = async (harness: Harness) => {
  const job = harness.search.pending.filter((pending) => pending.args.text === "smith 2020").at(-1)!;
  await act(async () => { job.complete({ revision: "1", results: [first] }); });
  await act(async () => { harness.preview.pending.at(-1)!.complete({ text: "[@a]" }); });
};

const selectedScenario = async (configure?: (harness: Harness) => void) => {
  const harness = await open((h) => {
    h.document.text = "Draft: smith 2020 ends.";
    h.document.selection = [7, 17];
    h.target.readsSelection = true;
    configure?.(h);
  });
  await act(async () => { await harness.reopen(); });
  return harness;
};

test("short selected text opens as the search, all selected, and Enter pastes over it", async () => {
  const harness = await selectedScenario();
  const box = searchBox() as HTMLInputElement;
  expect(box.value).toBe("smith 2020");
  expect([box.selectionStart, box.selectionEnd]).toEqual([0, 10]);
  await finishOpening(harness);
  await enter();
  await finishCopy(harness);
  await finishDelivery(harness);
  expect(harness.document.text).toBe("Draft: [@a] ends.");
  expectReceipt(harness, "Pasted into Example Writer", "[@a]");
});

test("typing replaces the selected search and Enter still replaces the document's selection", async () => {
  const harness = await selectedScenario();
  await type("something else");
  const job = harness.search.pending.filter((pending) => pending.args.text === "something else").at(-1)!;
  await act(async () => { job.complete({ revision: "1", results: [first] }); });
  await act(async () => { harness.preview.pending.at(-1)!.complete({ text: "[@a]" }); });
  await enter();
  await finishCopy(harness);
  await finishDelivery(harness);
  expect(harness.document.text).toBe("Draft: [@a] ends.");
});

test("a selection that changes before Enter is left alone, and the citation stays copied", async () => {
  const harness = await selectedScenario();
  await finishOpening(harness);
  harness.document.selection = [0, 5];
  await enter();
  await finishCopy(harness);
  await finishDelivery(harness);
  expect(harness.document.text).toBe("Draft: smith 2020 ends.");
  expect(harness.clipboard.text).toBe("[@a]");
  expectReceipt(harness, "Copied. Ibis replaces only text it showed you. Press ⌘V to replace it.", "[@a]");
});

test("a long selection is ignored: an empty search, no paste over it, and the footer says Copy", async () => {
  const words = "Invented sentence that runs on. ".repeat(6);
  const harness = await selectedScenario((h) => {
    h.document.text = words;
    h.document.selection = [0, words.length];
  });
  expect((searchBox() as HTMLInputElement).value).toBe("");
  await type("first");
  const job = harness.search.pending.filter((pending) => pending.args.text === "first").at(-1)!;
  await act(async () => { job.complete({ revision: "1", results: [first] }); });
  await act(async () => { harness.preview.pending.at(-1)!.complete({ text: "[@a]" }); });
  expect(screen.queryByRole("button", { name: /^Paste into/ })).toBeNull();
  await enter();
  await finishCopy(harness);
  await finishDelivery(harness);
  expect(harness.document.text).toBe(words);
  expectReceipt(harness, "Copied. Ibis replaces only text it showed you. Press ⌘V to replace it.", "[@a]");
});

const copyFirst = async (harness: Harness, query: string) => {
  await type(query);
  const job = harness.search.pending.filter((pending) => pending.args.text === query).at(-1)!;
  await act(async () => { job.complete({ revision: "1", results: [first] }); });
  await act(async () => { harness.preview.pending.at(-1)!.complete({ text: "[@a]" }); });
  await enter();
  await finishCopy(harness);
  await finishDelivery(harness);
};
const NOT_SHOWN = "Copied. Ibis replaces only text it showed you. Press ⌘V to replace it.";

test("a multi-line selection opens an empty search, and Enter leaves it alone", async () => {
  const lines = "first line\nsecond line";
  const harness = await selectedScenario((h) => {
    h.document.text = lines;
    h.document.selection = [0, 22];
  });
  expect((searchBox() as HTMLInputElement).value).toBe("");
  await copyFirst(harness, "first");
  expect(harness.document.text).toBe(lines);
  expectReceipt(harness, NOT_SHOWN, "[@a]");
});

test("a failed read opens an empty search, and Enter does not paste over text it never showed", async () => {
  const harness = await selectedScenario((h) => { h.target.selectionFails = true; });
  expect((searchBox() as HTMLInputElement).value).toBe("");
  await copyFirst(harness, "first");
  expect(harness.document.text).toBe("Draft: smith 2020 ends.");
  expect(harness.clipboard.text).toBe("[@a]");
  expectReceipt(harness, NOT_SHOWN, "[@a]");
});

test("a failed read with nothing selected still pastes at the caret", async () => {
  const harness = await selectedScenario((h) => {
    h.target.selectionFails = true;
    h.document.selection = [7, 7];
  });
  await copyFirst(harness, "first");
  expect(harness.document.text).toBe("Draft: [@a]smith 2020 ends.");
});

test("an app that cannot report a selection pastes as it always did", async () => {
  const harness = await selectedScenario((h) => { h.target.readsSelection = false; });
  expect((searchBox() as HTMLInputElement).value).toBe("");
  await copyFirst(harness, "first");
  expect(harness.document.text).toBe("Draft: [@a] ends.");
});

test("a refused app opens an empty search, and Enter changes nothing in the document", async () => {
  const harness = await selectedScenario((h) => { h.target.refusal = "terminal"; });
  expect((searchBox() as HTMLInputElement).value).toBe("");
  expect(harness.clipboard.owner).toBeNull();
  await copyFirst(harness, "first");
  expect(harness.document.text).toBe("Draft: smith 2020 ends.");
  expectReceipt(harness, "Copied. Not pasted into a terminal. Press ⌘V to paste.", "[@a]");
});

test("Copy only opens an empty search, and Enter changes nothing in the document", async () => {
  const harness = await selectedScenario((h) => {
    h.target.selectionFails = true;
    h.appChoices.set(h.target.appId, { output: null, paste: "copy-only" });
  });
  expect((searchBox() as HTMLInputElement).value).toBe("");
  await copyFirst(harness, "first");
  expect(harness.document.text).toBe("Draft: smith 2020 ends.");
  expectReceipt(harness, "Copied. Copy Only is set for this app. Press ⌘V to paste.", "[@a]");
});

test("an unapproved app opens an empty search and changes nothing in the document", async () => {
  const harness = await selectedScenario((h) => {
    h.target.selectionFails = true;
    h.target.verified = false;
  });
  expect((searchBox() as HTMLInputElement).value).toBe("");
  expect(harness.document.pastes).toEqual([]);
});

test("closing and reopening with nothing selected starts empty, not on the old document text", async () => {
  const harness = await selectedScenario();
  await act(async () => { fireEvent.keyDown(searchBox(), { key: "Escape" }); });
  expect((searchBox() as HTMLInputElement).value).toBe("");
  await act(async () => { fireEvent.keyDown(searchBox(), { key: "Escape" }); });
  harness.document.selection = [7, 7];
  await act(async () => { await harness.reopen(); });
  expect((searchBox() as HTMLInputElement).value).toBe("");
});

const pastedOnce = async () => {
  const harness = await selectedScenario((h) => { h.document.selection = [7, 7]; h.document.text = "Draft:  ends."; });
  await choose(harness, "first");
  await enter();
  await finishCopy(harness);
  await finishDelivery(harness);
  expect(harness.document.text).toBe("Draft: [@a] ends.");
  return harness;
};
const changeLast = async () => {
  const more = screen.getByRole("button", { name: /^More/ });
  await act(async () => { fireEvent.click(more); });
  await act(async () => { fireEvent.click(screen.getByRole("menuitem", { name: /Change Last Citation/ })); });
};

test("Change Last Citation replaces the old citation only when it is selected", async () => {
  const harness = await pastedOnce();
  harness.document.selection = [7, 11];
  await act(async () => { await harness.reopen(); });
  await changeLast();
  expect((searchBox() as HTMLInputElement).value).toBe("");
  await act(async () => { harness.preview.pending.at(-1)!.complete({ text: "[@a, p. 3]" }); });
  await enter();
  await finishCopy(harness);
  await finishDelivery(harness);
  expect(harness.document.text).toBe("Draft: [@a, p. 3] ends.");
  expectReceipt(harness, "Pasted into Example Writer", "[@a, p. 3]");
});

test("⌘E changes the last citation from inside More, and More closes", async () => {
  const harness = await pastedOnce();
  harness.document.selection = [7, 11];
  await act(async () => { await harness.reopen(); });
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: /^More/ })); });
  expect(screen.getByRole("menuitem", { name: /Change Last Citation/ })).toBeTruthy();
  await act(async () => { fireEvent.keyDown(document.activeElement ?? document.body, { key: "e", code: "KeyE", ctrlKey: true }); });
  expect(screen.queryByRole("menuitem", { name: /Change Last Citation/ })).toBeNull();
  expect(document.activeElement).toBe(searchBox());
  await act(async () => { harness.preview.pending.at(-1)!.complete({ text: "[@a, p. 3]" }); });
  await enter();
  await finishCopy(harness);
  await finishDelivery(harness);
  expect(harness.document.text).toBe("Draft: [@a, p. 3] ends.");
});

test("with nothing selected the changed citation is copied, the document is untouched and the notice says so", async () => {
  const harness = await pastedOnce();
  await act(async () => { await harness.reopen(); });
  await changeLast();
  await act(async () => { harness.preview.pending.at(-1)!.complete({ text: "[@a, p. 3]" }); });
  await enter();
  await finishCopy(harness);
  await finishDelivery(harness);
  expect(harness.document.text).toBe("Draft: [@a] ends.");
  expect(harness.clipboard.text).toBe("[@a, p. 3]");
  expectReceipt(harness, "Copied. Ibis replaces a citation only when it is selected. Select it, then press ⌘V.", "[@a, p. 3]");
});

test("the shortcut alone never reopens the last citation", async () => {
  const harness = await pastedOnce();
  await act(async () => { await harness.reopen(); });
  expect(harness.calls.filter((call) => call === "copy_file_citation")).toHaveLength(1);
  expect(screen.queryByText("Change Citation", { exact: false })).toBeNull();
  // Nothing from the last citation came back: the shelf has no reference, so
  // Enter on the empty search copies nothing.
  await enter();
  expect(harness.calls.filter((call) => call === "copy_file_citation")).toHaveLength(1);
});

test("More offers no Change Last Citation until this app has one", async () => {
  await open();
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: /^More/ })); });
  expect(screen.queryByRole("menuitem", { name: /Change Last Citation/ })).toBeNull();
});

test("the last citation is never written anywhere", async () => {
  const before = { ...window.localStorage };
  const harness = await pastedOnce();
  expect(JSON.stringify({ ...window.localStorage })).not.toContain("[@a]");
  expect(Object.keys(window.localStorage).filter((key) => !(key in before))).toEqual(expect.not.arrayContaining(["lastCitation"]));
  expect(harness.calls.filter((call) => /^(set_|save|write)/.test(String(call)))).toEqual([]);
});
