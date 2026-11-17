import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { App } from "./App";
import { backend, resetApp, result } from "./testing/app";
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
