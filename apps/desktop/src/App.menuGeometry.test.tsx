import { afterEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import * as api from "./api";
import { backend, resetApp, result } from "./testing/app";

afterEach(async () => { await resetApp(); vi.restoreAllMocks(); });

test("the citation shelf remains a separate non-scrolling band above the footer", async () => {
  backend({ search: () => [result("a", "Invented work")] });
  render(<App />);
  const preview = await screen.findByRole("region", { name: "Preview" });
  const shelf = preview.closest(".shelf")!;
  expect(shelf.parentElement?.tagName).toBe("MAIN");
  expect(shelf.closest(".picker-content")).toBeNull();
});

test.each(["error", "refusal"])("a window reset offers Undo without taking focus, and a native %s stays retryable", async (failure) => {
  const restored: unknown[] = [];
  let fail = true;
  const calls = backend({ undo_window_position: ({ revision }) => {
    restored.push(revision);
    if (fail && failure === "error") throw new Error("invented restore refusal");
    if (fail) return false;
    return true;
  } });
  render(<App />);
  await screen.findByText("Type to find a reference");
  const search = screen.getByLabelText("Search references");
  act(() => search.focus());
  await act(async () => { await emit("picker:window-reset", { revision: 7, undo: 7 }); });
  expect(screen.getByText("Window position reset.").getAttribute("role")).toBe("status");
  expect(document.activeElement).toBe(search);
  fireEvent.click(screen.getByRole("button", { name: "Undo" }));
  expect((await screen.findByRole("alert")).textContent).toBe("Could not undo. Try again.");
  fail = false;
  fireEvent.click(screen.getByRole("button", { name: "Undo" }));
  await waitFor(() => expect(screen.queryByRole("button", { name: "Undo" })).toBeNull());
  expect(restored).toEqual([7, 7]);
  expect(calls).not.toContain("fit_picker");
});

test("More is closed while its exit overlaps the probe's keys-sheet and Copy as steps", async () => {
  backend({ search: () => [] });
  render(<App />);
  await screen.findByText("Type to find a reference");
  const more = screen.getByRole("button", { name: "More" });
  const search = screen.getByRole("combobox");
  fireEvent.click(more);
  const moreMenu = await screen.findByRole("menu", { name: "More" });
  const popover = moreMenu.closest(".menu-popover")!;
  let finishExit!: () => void;
  const finished = new Promise<void>((resolve) => { finishExit = resolve; });
  Object.defineProperty(popover, "getAnimations", { value: () => popover.hasAttribute("data-exiting")
    ? [{ playState: "running", finished }] : [] });
  try {
    fireEvent.keyDown(moreMenu, { key: "End" });
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("menuitem", { name: /^Quit/ })));
    fireEvent.keyDown(document.activeElement!, { key: "Home" });
    fireEvent.keyDown(document.activeElement!, { key: "Escape" });
    expect(more.getAttribute("aria-expanded")).toBe("false");
    expect(popover.hasAttribute("data-exiting")).toBe(true);
    // Playwright focuses its search locator before sending the shortcut.
    act(() => search.focus());
    fireEvent.keyDown(search, { key: "/", ctrlKey: true });
    const keys = await screen.findByRole("dialog", { name: "Keyboard shortcuts" });
    fireEvent.keyDown(keys, { key: "Escape" });
    const format = screen.getByRole("button", { name: /^Copy as/ });
    fireEvent.click(format);
    const formatMenu = await screen.findByRole("menu", { name: /^Copy as/ });
    // The browser can still find the closing DOM with an unqualified locator.
    // Presence during exit does not mean both menu triggers are open.
    expect(document.querySelectorAll('[role="menu"]')).toHaveLength(2);
    expect(more.getAttribute("aria-expanded")).toBe("false");
    expect(popover.hasAttribute("data-exiting")).toBe(true);
    expect(format.getAttribute("aria-expanded")).toBe("true");
    await act(async () => { finishExit(); });
    await waitFor(() => expect(moreMenu.isConnected).toBe(false));
    expect(formatMenu.isConnected).toBe(true);
  } finally {
    await act(async () => { finishExit(); });
  }
});

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

test("clearing a settings notice keeps the pointer's newer place even before its scroll event", async () => {
  const library: api.LibraryStatus = {
    state: "loaded", revision: "1", has_snapshot: true, zotero: false, zotero_libraries: [],
    name: "invented.bib", snapshot_name: "invented.bib", references: 14, source_counts: {},
    warnings: 0, error: null, problem: null, note: null, settings_notice: null,
    snapshot_zotero: false, stale: false, updated_at: null, checked_at: null,
  };
  const found = Array.from({ length: 14 }, (_, i) => result(`synthetic${i}`, `Invented reference ${i}`));
  backend({ library_status: () => library, search: ({ query }) => query.text ? found : [] });
  let changed!: (status: api.LibraryStatus) => void;
  const hasNotice = (node: HTMLElement) => !!node.closest("main")?.querySelector(".notices [role=alert]");
  const computed = window.getComputedStyle;
  vi.spyOn(window, "getComputedStyle").mockImplementation((node) => {
    if (!node.matches(".results-area, .picker-content")) return computed(node);
    const style = document.createElement("div").style;
    style.overflowY = node.matches(".results-area") && hasNotice(node as HTMLElement) ? "visible" : "auto";
    return style;
  });
  const scrollTop = Object.getOwnPropertyDescriptor(Element.prototype, "scrollTop")!;
  vi.spyOn(HTMLElement.prototype, "scrollTop", "get").mockImplementation(function (this: HTMLElement) {
    if (this.matches(".results-area") && hasNotice(this) || this.matches(".picker-content") && !hasNotice(this)) scrollTop.set!.call(this, 0);
    return scrollTop.get!.call(this);
  });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.matches(".notices")) return new DOMRect(0, 53, 720, hasNotice(this) ? 42 : 0);
    if (this.matches("[role=option]")) {
      const middle = this.closest(".picker-content")!;
      const area = middle.querySelector<HTMLElement>(".results-area")!;
      return new DOMRect(8, 89 + Number(this.id.slice(7)) * 60 + (hasNotice(this) ? 42 : 0) - area.scrollTop - middle.scrollTop, 704, 58);
    }
    return new DOMRect();
  });
  render(<App api={{ ...api, onLibraryChanged: async (handler) => { changed = handler; return () => {}; } }} />);
  await screen.findByText("Type to find a reference");
  const search = screen.getByRole<HTMLInputElement>("combobox");
  fireEvent.change(search, { target: { value: "invented" } });
  const row = await screen.findByRole("option", { name: /Invented reference 8/ });
  const area = document.querySelector<HTMLElement>(".results-area")!;
  const middle = document.querySelector<HTMLElement>(".picker-content")!;
  area.scrollTop = 380; fireEvent.scroll(area);
  fireEvent.click(row, { detail: 1 });
  search.setSelectionRange(2, 5);
  expect(row.getBoundingClientRect().top).toBe(189);
  await act(async () => changed({ ...library, settings_notice: "Synthetic asynchronous settings notice." }));
  expect(row.getBoundingClientRect().top).toBe(189);
  middle.scrollTop += 60;
  expect(row.getBoundingClientRect().top).toBe(129);
  await act(async () => changed(library));
  // The unchanged asynchronous search/preview replies must preserve this too.
  expect(screen.queryByRole("alert")).toBeNull();
  expect(row.getAttribute("aria-selected")).toBe("true");
  expect(row.getBoundingClientRect().top).toBe(129);
  expect([area.scrollTop, middle.scrollTop]).toEqual([440, 0]);
  expect(document.activeElement).toBe(search);
  expect([search.selectionStart, search.selectionEnd]).toEqual([2, 5]);
  fireEvent.scroll(middle);
  expect(row.getBoundingClientRect().top).toBe(129);
});

test("resizing while editing keeps the selected title readable without moving Page focus", async () => {
  backend({ search: () => [result("a", "First invented work")] });
  render(<App />);
  await screen.findByRole("option", { name: /First invented work/ });
  const search = screen.getByRole("combobox");
  fireEvent.keyDown(search, { key: "Enter", shiftKey: true });
  fireEvent.click(screen.getByRole("button", { name: "Edit citation" }));
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
