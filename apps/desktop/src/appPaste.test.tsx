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
  await act(async () => { fireEvent.click(screen.getByRole("menuitem", { name: label })); });
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
  expect(screen.getByRole("menuitemradio", { name: "APA · text" })).toBeTruthy();
  expect(h.copy.calls).toHaveLength(0);
  await answer("Paste into Example Writer from now on");
  expect(h.copy.calls).toHaveLength(0);
  expect(h.document.text).toBe("Draft: [citation].");
  expect(screen.queryByRole("menu")).toBeNull();
  await enter();
  await finish(h);
  expect(h.document.text).toBe("Draft: [@a].");
  expect(h.document.pastes).toHaveLength(1);
  expect(receipt(h)).toBe("Pasted into Example Writer");
  await act(async () => { await h.reopen(); });
  await enter();
  expect(screen.queryByRole("menu")).toBeNull();
  await finish(h);
  expect(h.document.pastes).toHaveLength(2);
});

test("Copy only is remembered across reopening and its receipt stays true", async () => {
  const h = await open();
  await enter();
  await answer("Copy only");
  expect(h.copy.calls).toHaveLength(0);
  await enter();
  await finish(h);
  expect(h.clipboard.text).toBe("[@a]");
  expect(h.document.pastes).toHaveLength(0);
  expect(receipt(h)).toBe("Copied. Copy only is set for this app");
  await act(async () => { await h.reopen(); });
  await enter();
  expect(screen.queryByRole("menu")).toBeNull();
  await finish(h);
  expect(h.document.pastes).toHaveLength(0);
});

test("More can keep a recognised writing app to copying, with a fresh Enter", async () => {
  const h = await open((h) => { h.target.verified = true; });
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: /^More/ })); });
  await answer("Copy only in Example Writer");
  await act(async () => { await vi.advanceTimersByTimeAsync(200); });
  expect(h.copy.calls).toHaveLength(0);
  await enter();
  await finish(h);
  expect(h.document.pastes).toHaveLength(0);
  expect(receipt(h)).toBe("Copied. Copy only is set for this app");
});

test("an unknown app without a proven target only copies and never asks for approval", async () => {
  const h = await open((h) => { h.target.window = null; });
  await enter();
  expect(screen.queryByRole("menu")).toBeNull();
  await finish(h);
  expect(h.document.pastes).toHaveLength(0);
  expect(h.appChoices.size).toBe(0);
  expect(receipt(h)).toBe("Copied. Automatic paste is not approved for this app");
});

test.each(["Automatic", "Forget formats for apps"])("%s resets unknown-app approval and brings the question back", async (reset) => {
  const h = await open();
  await enter();
  await answer("Paste into Example Writer from now on");
  if (reset === "Automatic") {
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: /^Copy as / })); });
    await answer(reset);
  } else {
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: /^More/ })); });
    await answer(reset);
    await act(async () => { await vi.advanceTimersByTimeAsync(200); });
  }
  expect(h.copy.calls).toHaveLength(0);
  await enter();
  expect(screen.getByRole("menuitem", { name: "Paste into Example Writer from now on" })).toBeTruthy();
  expect(h.document.pastes).toHaveLength(0);
});

test("dismissing the unknown-app question or picking a format never grants paste", async () => {
  const h = await open();
  await enter();
  await act(async () => { fireEvent.keyDown(screen.getByRole("searchbox"), { key: "Escape" }); });
  expect(h.copy.calls).toHaveLength(0);
  await enter();
  await act(async () => { fireEvent.click(screen.getByRole("menuitemradio", { name: "Pandoc key" })); });
  expect(h.copy.calls).toHaveLength(0);
  await enter();
  expect(screen.getByRole("menuitem", { name: "Copy only" })).toBeTruthy();
  expect(h.document.pastes).toHaveLength(0);
});

test("Just this once stays in the chooser and paste approval preserves that temporary citation", async () => {
  const h = await open();
  await enter();
  await act(async () => { fireEvent.click(screen.getByRole("menuitemcheckbox", { name: "Just this once" })); });
  expect(screen.getByRole("menuitemcheckbox", { name: "Just this once", checked: true })).toBeTruthy();
  await act(async () => { fireEvent.click(screen.getByRole("menuitemradio", { name: "Typst key" })); });
  await act(async () => { h.preview.pending[0]!.complete({ text: "@a" }); });
  expect(h.copy.calls).toHaveLength(0);
  await enter();
  await answer("Paste into Example Writer from now on");
  expect(h.appChoices.get("example.writer")).toEqual({ output: null, paste: "paste" });
  expect(h.copy.calls).toHaveLength(0);
  await enter();
  await finish(h);
  expect(h.document.text).toBe("Draft: @a.");
  expect(h.document.pastes).toHaveLength(1);
  expect(receipt(h)).toBe("Pasted into Example Writer");
});

test.each(["terminal", "remote-desktop", "virtual-machine", "nonstandard-shortcut"] as const)("a refused %s app never asks, even with a saved paste choice", async (reason) => {
  const h = await open((h) => { h.target.refusal = reason; h.appChoices.set("example.writer", { output: { key: "pandoc" }, paste: "paste" }); });
  await enter();
  expect(screen.queryByRole("menu")).toBeNull();
  await finish(h);
  expect(h.document.pastes).toHaveLength(0);
  expect(receipt(h)).toMatch(/^Copied\./);
});

test("a failed approval save keeps asking and cannot release the held Enter", async () => {
  const h = await open((h) => { h.holdAppSaves = true; });
  await enter();
  await answer("Paste into Example Writer from now on");
  await act(async () => { h.appSaves.pending[0]!.fail(new Error("invented write failure")); });
  expect(h.copy.calls).toHaveLength(0);
  await enter();
  expect(screen.getByRole("menuitem", { name: "Paste into Example Writer from now on" })).toBeTruthy();
  expect(h.document.pastes).toHaveLength(0);
});

test("a delayed approval cannot replay its held citation after reopening in another app", async () => {
  const h = await open((h) => { h.holdAppSaves = true; });
  await enter();
  await answer("Paste into Example Writer from now on");
  h.document.app = "Other Writer";
  h.target.appId = "example.other";
  h.target.verified = true;
  await act(async () => { await h.reopen(); });
  await enter();
  expect(h.copy.calls).toHaveLength(0);
  await act(async () => { h.appSaves.pending[0]!.complete(); });
  await finish(h);
  expect(h.document.pastes).toHaveLength(1);
  expect(receipt(h)).toBe("Pasted into Other Writer");
  expect(h.copy.calls).toHaveLength(1);
  expect(h.appChoices.has("example.other")).toBe(false);
});

test("close and reopen while saving approval never replays Enter", async () => {
  const h = await open((h) => { h.holdAppSaves = true; });
  await enter();
  await answer("Paste into Example Writer from now on");
  await act(async () => { await h.reopen(); });
  await act(async () => { h.appSaves.pending[0]!.complete(); });
  expect(h.copy.calls).toHaveLength(0);
  expect(h.document.pastes).toHaveLength(0);
  await enter();
  await finish(h);
  expect(h.document.pastes).toHaveLength(1);
});
