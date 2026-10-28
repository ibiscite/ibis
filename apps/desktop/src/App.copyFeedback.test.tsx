import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { App } from "./App";
import { backend, resetApp, result } from "./testing/app";

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

async function toggleKeepOpen() {
  await act(async () => {
    fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "P", code: "KeyP", ctrlKey: true, shiftKey: true });
  });
}

test.each(["Enter", " "])("copy confirmation expires, but copy warnings remain until dismissed with %s", async (key) => {
  let warnings: string[] = [];
  const calls = await openPicker({ copy_file_citation: () => ({ text: "@a", warnings }) });
  await copy();
  expect(screen.getByText("Copied", { selector: ".copy-notice p" })).toBeTruthy();
  await act(() => vi.advanceTimersByTimeAsync(2399));
  expect(document.querySelector(".copy-notice")).toBeTruthy();
  await act(() => vi.advanceTimersByTimeAsync(1));
  expect(document.querySelector(".copy-notice")).toBeNull();

  warnings = ["A warning to read"];
  await copy();
  await act(() => vi.advanceTimersByTimeAsync(3000));
  expect(screen.getByText("A warning to read")).toBeTruthy();
  const dismiss = screen.getByRole("button", { name: "Dismiss copy warnings" });
  const actions = () => calls.filter((command) => command === "copy_file_citation" || command === "hide_picker");
  const previousActions = actions();
  act(() => dismiss.focus());
  const code = key === " " ? "Space" : "Enter";
  fireEvent.keyDown(dismiss, { key, code });
  fireEvent.keyDown(dismiss, { key, code, repeat: true });
  fireEvent.keyUp(dismiss, { key, code });
  expect(screen.queryByLabelText("Copy warnings")).toBeNull();
  expect(actions()).toEqual(previousActions);
});

test("copy remains confirmed when hiding after a successful copy fails", async () => {
  await openPicker({ copy_file_citation: () => ({ text: "@a", warnings: [] }),
    hide_picker: () => { throw new Error("SYNTHETIC_PRIVATE_PAYLOAD"); } });
  await copy();
  expect(screen.getByText("Ibis could not hide this window. Try again.")).toBeTruthy();
  expect(screen.getByText("Copied", { selector: ".copy-notice p" })).toBeTruthy();
  expect(document.body.textContent).not.toContain("Nothing was copied");
  expect(document.body.textContent).not.toContain("SYNTHETIC_PRIVATE_PAYLOAD");
});

test("a clean copy hides the picker, then says Copied with the copied text", async () => {
  let finishCopy!: () => void;
  let finishHide!: () => void;
  const notices: unknown[] = [];
  const calls = await openPicker({
    copy_file_citation: () => new Promise((resolve) => { finishCopy = () => resolve({ text: "[@a]", warnings: [] }); }),
    hide_picker: () => new Promise<void>((resolve) => { finishHide = resolve; }),
    show_copied_notice: (args) => { notices.push(args); return null; },
  });
  await copy();
  expect(calls).toContain("copy_file_citation");
  expect(calls).not.toContain("hide_picker");
  expect(document.querySelector(".copy-notice")).toBeNull();
  await act(async () => { finishCopy(); });
  expect(calls).toContain("hide_picker");
  expect(notices).toEqual([]);
  expect(screen.getByText("Copied", { selector: ".copy-notice p" })).toBeTruthy();
  await act(async () => { finishHide(); });
  expect(notices).toEqual([{ text: "[@a]" }]);
  expect(calls.indexOf("hide_picker")).toBeLessThan(calls.indexOf("show_copied_notice"));
});

test("copies with warnings, pinned copies and failed hides show no Copied notice window", async () => {
  let warnings = ["bare keys cannot carry page numbers"];
  let hideFails = false;
  const calls = await openPicker({
    copy_file_citation: () => ({ text: "a", warnings }),
    hide_picker: () => { if (hideFails) throw new Error("SYNTHETIC"); return null; },
  });
  await copy();
  expect(screen.getByLabelText("Copy warnings")).toBeTruthy();
  await toggleKeepOpen();
  warnings = [];
  await copy();
  expect(calls.filter((c) => c === "copy_file_citation")).toHaveLength(2);
  expect(screen.queryByLabelText("Copy warnings")).toBeNull();
  expect(calls).not.toContain("hide_picker");
  await toggleKeepOpen();
  hideFails = true;
  await copy();
  expect(screen.getByText("Ibis could not hide this window. Try again.")).toBeTruthy();
  expect(calls).not.toContain("show_copied_notice");
});

test("a failed Copied notice never undoes the copy or the hide", async () => {
  const log = vi.spyOn(console, "error").mockImplementation(() => undefined);
  try {
    const calls = await openPicker({
      copy_file_citation: () => ({ text: "@a", warnings: [] }),
      show_copied_notice: () => { throw new Error("SYNTHETIC_PRIVATE_PAYLOAD"); },
    });
    await copy();
    expect(log).toHaveBeenCalledWith("The Copied notice could not be shown.");
    expect(calls).toContain("hide_picker");
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText("Copied", { selector: ".copy-notice p" })).toBeTruthy();
    expect(log.mock.calls.flat().join(" ")).not.toContain("SYNTHETIC_PRIVATE_PAYLOAD");
  } finally { log.mockRestore(); }
});
