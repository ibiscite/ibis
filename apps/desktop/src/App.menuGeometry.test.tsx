import { afterEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { App } from "./App";
import * as api from "./api";
import { backend, resetApp, result } from "./testing/app";

afterEach(async () => { await resetApp(); vi.restoreAllMocks(); });

test.each([false, true])("empty, controls, results, selection, no matches and keys never measure or resize (Larger=%s)", async (larger) => {
  const calls = backend({
    picker_preferences: () => ({ citation_style: null, density: "comfortable", larger_text: larger, show_paste_preview: true, search_mark: false, paste: false }),
    search: ({ query }) => query.text === "work" ? [result("a", "First invented work")] : [],
  });
  const height = vi.spyOn(HTMLElement.prototype, "offsetHeight", "get");
  await act(async () => { render(<App />); });
  await screen.findByText("Type to find a reference");
  const search = screen.getByRole("combobox");
  for (const layer of ["More", "output"]) {
    if (layer === "More") fireEvent.keyDown(search, { key: "k", ctrlKey: true });
    else fireEvent.click(screen.getByRole("button", { name: /^Copy as/ }));
    const menu = await screen.findByRole("menu");
    fireEvent.keyDown(layer === "More" ? menu : screen.getByRole("searchbox"), { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
  }
  fireEvent.click(screen.getByRole("button", { name: "Keep Window Open" }));
  fireEvent.change(search, { target: { value: "work" } });
  await screen.findByRole("option", { name: /First invented work/ });
  fireEvent.keyDown(search, { key: "Enter", shiftKey: true });
  expect(document.querySelector("main")?.hasAttribute("data-selection")).toBe(true);
  fireEvent.change(search, { target: { value: "nothing" } });
  await screen.findByText("No matching references");
  fireEvent.click(screen.getByRole("button", { name: "Shortcuts" }));
  await screen.findByRole("dialog", { name: "Keyboard shortcuts" });
  expect(calls).not.toContain("fit_picker");
  expect(calls.some((call) => /set_size|set_position/.test(call))).toBe(false);
  expect(height.mock.contexts.filter((node) => (node as HTMLElement)?.tagName === "MAIN")).toHaveLength(0);
});

test("a failed size save is visible and a later successful save clears it without sizing IPC", async () => {
  const calls = backend();
  let saved: (success: boolean) => void = () => {};
  render(<App api={{ ...api, onWindowSizeSaved: async (handler) => { saved = handler; return () => {}; } }} />);
  await screen.findByText("Type to find a reference");
  act(() => saved(false));
  expect(screen.getByRole("alert").textContent).toMatch(/save the window size/);
  act(() => saved(true));
  expect(screen.queryByRole("alert")).toBeNull();
  expect(calls).not.toContain("fit_picker");
});

test("resizing while editing keeps the selected title readable without moving Page focus", async () => {
  backend({ search: () => [result("a", "First invented work")] });
  render(<App />);
  await screen.findByRole("option", { name: /First invented work/ });
  const search = screen.getByRole("combobox");
  fireEvent.keyDown(search, { key: "Enter", shiftKey: true });
  const page = screen.getByLabelText("Page", { exact: true });
  act(() => page.focus());
  const middle = document.querySelector<HTMLElement>(".picker-content")!;
  const computed = window.getComputedStyle;
  vi.spyOn(window, "getComputedStyle").mockImplementation((node) => {
    if (!node.matches(".results-area, .picker-content")) return computed(node);
    const style = document.createElement("div").style;
    style.overflowY = node === middle ? "auto" : "visible";
    return style;
  });
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this === middle ? 93 : 0;
  });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this === middle) return new DOMRect(0, 185, 480, 93);
    if (this.matches('[role="option"], .result-title')) return new DOMRect(8, 241 - middle.scrollTop, 464, this.matches(".result-title") ? 80 : 116);
    return new DOMRect();
  });
  // The 200% minimum frame has less room than a whole two-line result.
  // Revealing its title must also work while the editor owns keyboard focus.
  expect(middle.clientHeight).toBe(93);
  expect(getComputedStyle(middle).overflowY).toBe("auto");
  expect(document.getElementById("result-0")).not.toBeNull();
  fireEvent.resize(window);
  const title = document.querySelector(".result-title")!.getBoundingClientRect();
  expect(title.top).toBeGreaterThanOrEqual(185);
  expect(title.bottom).toBeLessThanOrEqual(278);
  expect(document.activeElement).toBe(page);
});

test("startup shortcut notices belong to the middle scroller beside other notices", async () => {
  backend({ shortcut_status: () => ({ shortcut: "Alt+Y", registered: true, error: "Synthetic shortcut refusal" }) });
  render(<App />);
  const notice = await screen.findByText(/Your shortcut setting was not used/);
  expect(notice.closest(".picker-content")).not.toBeNull();
  expect(notice.closest(".notices")).not.toBeNull();
});
