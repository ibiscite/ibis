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

test("the bar names no action until the opening's offer says what Enter does, then the practice", async () => {
  let finishOffer!: () => void;
  const calls = backend({
    search: () => [result("aster2026", "Invented gardens")],
    delight_view: () => new Promise((resolve) => { finishOffer = () => resolve({ practice: true, status: null, peck: false }); }),
    preview_file_citation: () => receipt("[@aster2026]"),
  });
  await act(async () => { render(<App />); });
  // The preview is ready first: the action keeps its room, but says nothing Enter may not do.
  expect(screen.queryByRole("button", { name: /^(Copy|Paste)/ })).toBeNull();
  expect(document.querySelector(".footer-action")?.getAttribute("aria-hidden")).toBe("true");
  await act(async () => { finishOffer(); });
  expect(screen.getByRole("button", { name: "Paste Here to Try" })).toBeTruthy();
  await enter();
  expect(calls).not.toContain("deliver_citation");
  expect(screen.getByText(/\[@aster2026\].*pasted here/)).toBeTruthy();
  // After the practice, the bar and Enter both deliver.
  expect(screen.queryByRole("button", { name: "Paste Here to Try" })).toBeNull();
});

test("an editor's request is sent, never turned into the practice, while the offer is still being read", async () => {
  let finish!: (view: unknown) => void;
  const calls = backend({
    search: () => [result("a", "Invented A")],
    editor_request: () => ({ request: 1, format: "Pandoc" }),
    delight_view: () => new Promise((resolve) => { finish = resolve; }),
    preview_file_citation: () => receipt("[@a]"),
  });
  await act(async () => { render(<App />); });
  expect(screen.getByRole("button", { name: "Send to Editor" })).toBeTruthy();
  await enter();
  await act(async () => { finish({ practice: true, status: null, peck: false }); });
  expect(calls).toContain("deliver_citation");
  expect(screen.queryByText(/pasted here/)).toBeNull();
});

test("a refusal is said for Enter on that citation only, never carried to another", async () => {
  const reason = "A key in thesis.bib identifies a different work.";
  const calls = backend({
    search: ({ query }) => [result(query.text || "a", `Invented ${query.text || "a"}`)],
    preview_file_citation: ({ items }) => ({ ...receipt(`[@${items[0]!.id.key}]`), project_refusal: items[0]!.id.key === recordId("b") ? null : reason }),
  });
  await act(async () => { render(<App />); });
  await enter();
  await act(async () => { fireEvent.change(searchBox(), { target: { value: "b" } }); });
  await enter();
  expect(calls.filter((call) => call === "deliver_citation")).toHaveLength(1);
  const added: string[] = [];
  const observer = new MutationObserver((records) => { for (const record of records) for (const node of record.addedNodes) added.push(node.textContent ?? ""); });
  observer.observe(document.querySelector(".delivery-bar")!, { childList: true, subtree: true });
  await act(async () => { fireEvent.change(searchBox(), { target: { value: "c" } }); });
  observer.disconnect();
  expect(added).not.toContain(`Not pasted or copied. ${reason}`);
});
