import { recordId } from "./test-fixtures";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { App } from "./App";
import { backend, prepared, receipt, resetApp, result } from "./testing/app";

beforeEach(() => vi.useFakeTimers());
afterEach(resetApp);
const searchBox = () => screen.getByLabelText("Search references");
const enter = async () => { await act(async () => { fireEvent.keyDown(searchBox(), { key: "Enter" }); }); };

test("Enter during a new query waits for current results and preview, once", async () => {
  let finishSearch!: (value: unknown) => void;
  let finishPreview!: (value: ReturnType<typeof receipt>) => void;
  const copied: string[] = [];
  backend({
    search: ({ query }) => !query.text ? [result("old", "Old result")]
      : new Promise((resolve) => { finishSearch = resolve; }),
    preview_file_citation: ({ items }) => items[0]!.id.key === recordId("old") ? receipt("@old")
      : new Promise((resolve) => { finishPreview = resolve; }),
    deliver_citation: (args) => { const { preview } = prepared(args); copied.push(preview.text); return { outcome: "retained", copied: { text: preview.text, warnings: [] }, reason: "pinned" }; },
  });
  await act(async () => { render(<App />); });
  await act(async () => { fireEvent.change(searchBox(), { target: { value: "new" } }); });
  await enter();
  await enter();
  expect(copied).toEqual([]);
  await act(async () => { finishSearch([result("new", "New result")]); });
  expect(copied).toEqual([]);
  await act(async () => { finishPreview(receipt("@new")); });
  expect(copied).toEqual(["@new"]);
});

test("Enter on the practice prompt waits for the preview and pastes only in Ibis", async () => {
  let finishPreview!: (value: ReturnType<typeof receipt>) => void;
  const preview = new Promise<ReturnType<typeof receipt>>((resolve) => { finishPreview = resolve; });
  const calls = backend({
    search: () => [result("aster2026", "Invented gardens")],
    delight_view: () => ({ practice: true, status: null, peck: false }),
    preview_file_citation: () => preview,
  });
  await act(async () => { render(<App />); });
  expect(screen.getByText("Try a paste here first: press Enter.")).toBeTruthy();
  expect(calls).toContain("preview_file_citation");

  await enter();
  await act(async () => { finishPreview(receipt("[@aster2026]")); });
  expect(calls).not.toContain("deliver_citation");
  expect(screen.getByText(/\[@aster2026\].*pasted here/)).toBeTruthy();

  await enter();
  expect(calls.filter((call) => call === "deliver_citation")).toHaveLength(1);
});

test.each(["offer", "preview"] as const)("first Enter stays in Ibis when the %s replies first", async (first) => {
  let finishOffer!: () => void;
  let finishPreview!: () => void;
  const calls = backend({
    search: () => [result("aster2026", "Invented gardens")],
    delight_view: () => new Promise((resolve) => { finishOffer = () => resolve({ practice: true, status: null, peck: false }); }),
    preview_file_citation: () => new Promise((resolve) => { finishPreview = () => resolve(receipt("[@aster2026]")); }),
  });
  await act(async () => { render(<App />); });
  expect(calls).toContain("delight_view");
  expect(calls).toContain("preview_file_citation");

  await enter();
  await act(async () => { (first === "offer" ? finishOffer : finishPreview)(); });
  if (first === "offer") expect(screen.getByText("Try a paste here first: press Enter.")).toBeTruthy();
  expect(calls).not.toContain("deliver_citation");
  await act(async () => { (first === "offer" ? finishPreview : finishOffer)(); });
  expect(calls).not.toContain("deliver_citation");
  expect(screen.getByText(/\[@aster2026\].*pasted here/)).toBeTruthy();

  await enter();
  expect(calls.filter((call) => call === "deliver_citation")).toHaveLength(1);
});
