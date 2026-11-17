import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { App } from "./App";
import { resetApp, result } from "./testing/app";
import { deliveryHarness } from "./testing/delivery";

beforeEach(() => vi.useFakeTimers());
afterEach(resetApp);
type Harness = ReturnType<typeof deliveryHarness>;
const search = () => screen.getByRole("combobox", { name: "Search references" });
const enter = async () => { await act(async () => { fireEvent.keyDown(search(), { key: "Enter" }); }); };

async function open(configure?: (h: Harness) => void) {
  const h = deliveryHarness();
  h.target.verified = false;
  configure?.(h);
  await act(async () => { render(<App />); });
  await act(async () => { h.search.pending[0]!.complete({ revision: "1", results: [result("a", "Invented reference")] }); });
  await act(async () => { h.preview.pending[0]!.complete({ text: "[@a]" }); });
  return h;
}
async function answer(label: string) {
  await act(async () => { fireEvent.click(screen.getByRole("option", { name: label })); });
}
async function reopen(h: Harness) {
  await act(async () => { await h.reopen(); });
  // The opening snapshot already contains the current recent rows.
  await act(async () => { h.preview.pending.at(-1)!.complete({ text: "[@a]" }); });
}
async function finish(h: Harness) {
  await act(async () => { h.copy.pending[0]!.complete(); });
  await act(async () => { h.delivery.pending[0]!.complete(); });
  await act(async () => { h.notice.pending[0]!.complete(); });
}
const receipt = (h: Harness) => within(h.receipt).getByRole("status").querySelector("strong")?.textContent;

test("the first unknown-app Enter asks in the format chooser; approval needs fresh Enter and is remembered", async () => {
  const h = await open();
  await enter();
  expect(screen.getByRole("option", { name: "APA" })).toBeTruthy();
  expect(h.copy.calls).toHaveLength(0);
  await answer("Paste into Example Writer from Now On");
  expect(h.copy.calls).toHaveLength(0);
  expect(h.document.text).toBe("Draft: [citation].");
  expect(screen.queryByRole("dialog")).toBeNull();
  await enter();
  await finish(h);
  expect(h.document.text).toBe("Draft: [@a].");
  expect(h.document.pastes).toHaveLength(1);
  expect(receipt(h)).toBe("Pasted into Example Writer");
  await reopen(h);
  await enter();
  expect(screen.queryByRole("dialog")).toBeNull();
  await finish(h);
  expect(h.document.pastes).toHaveLength(2);
});
