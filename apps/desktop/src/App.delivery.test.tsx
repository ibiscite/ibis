import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { App } from "./App";
import { resetApp, result } from "./testing/app";
import { deliveryHarness } from "./testing/delivery";

beforeEach(() => { vi.useFakeTimers(); });
afterEach(resetApp);

type Harness = ReturnType<typeof deliveryHarness>;
const first = result("a", "First invented reference");
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

test("Enter waits for a preview, then copies and replaces the document selection exactly once", async () => {
  const harness = await open();
  await type("first");
  await act(async () => { harness.search.pending[0]!.complete({ revision: "1", results: [first] }); });
  await enter();
  await enter(false, true);
  expect(harness.clipboard.owner).toBeNull();
  expect(harness.document.text).toBe("Draft: [citation].");
  await act(async () => { harness.preview.pending[0]!.complete({ text: "[@a]", clipboardHtml: "<i>[@a]</i>" }); });
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
  await act(async () => { fireEvent.click(screen.getByRole("option", { name: /Change Last Citation/ })); });
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
