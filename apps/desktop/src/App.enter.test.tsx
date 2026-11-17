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
