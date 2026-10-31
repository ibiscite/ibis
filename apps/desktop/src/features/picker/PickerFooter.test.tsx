import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { PickerFooter } from "./PickerFooter";

afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); document.documentElement.dir = ""; });

function geometry() {
  let available = 720;
  const callbacks: (() => void)[] = [];
  vi.stubGlobal("ResizeObserver", class { constructor(callback: () => void) { callbacks.push(callback); } observe() {} disconnect() {} });
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(function (this: HTMLElement) { return this.matches(".library-bar") ? available : 0; });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const size = this.matches(".footer-status-size") ? 224 : this.matches(".footer-status-minimum") ? 44
      : this.matches("button[aria-label='More']") ? 100 : this.matches("button") ? 180 : 0;
    return new DOMRect(0, 0, size, 24);
  });
  return (width: number) => { available = width; act(() => callbacks.forEach((callback) => callback())); };
}

function footer(copy = vi.fn(), altHeld = false) {
  return <PickerFooter library={null} groupName={String} hints={{ enter: "Copy 20 as Pandoc", shiftEnter: "Add to citation", escape: "Hide" }}
    altHeld={altHeld} actions={{ copy, add: copy, escape: copy }} more={<button aria-label="More">More</button>} />;
}

test("the footer is status, More, Copy with no Shortcuts button at any width", () => {
  const resize = geometry(); render(footer());
  const bar = document.querySelector(".library-bar")!;
  const more = screen.getByRole("button", { name: "More" });
  const copy = screen.getByRole("button", { name: "Copy 20 as Pandoc" });
  expect(more.compareDocumentPosition(copy) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(copy.getAttribute("data-variant")).toBe("primary");
  for (const width of [720, 440, 300, 900]) {
    resize(width);
    expect(screen.queryByRole("button", { name: "Shortcuts" })).toBeNull();
    expect(screen.getByRole("button", { name: "More" })).toBe(more);
    expect(bar.textContent).toContain("Library details unavailable");
  }
});

test("status shortens first, then labels yield while Copy keeps its name, key and focus", () => {
  const resize = geometry(); const copy = vi.fn(); render(footer(copy));
  const bar = document.querySelector(".library-bar")!;
  const action = screen.getByRole("button", { name: "Copy 20 as Pandoc" });
  act(() => action.focus());
  resize(440);
  expect(bar.hasAttribute("data-status-compact")).toBe(true);
  expect(bar.hasAttribute("data-hints-compact")).toBe(false);
  resize(300);
  expect(bar.hasAttribute("data-hints-compact")).toBe(true);
  expect(action.getAttribute("aria-keyshortcuts")).toBe("Enter");
  expect(document.activeElement).toBe(action);
  fireEvent.click(action);
  expect(copy).toHaveBeenCalledOnce();
  resize(900);
  expect(bar.hasAttribute("data-status-compact")).toBe(false);
  expect(bar.hasAttribute("data-hints-compact")).toBe(false);
});

test("RTL and Alt preserve the guarded Add and Escape actions without a shortcut hint", () => {
  const resize = geometry(); document.documentElement.dir = "rtl";
  const action = vi.fn(); render(footer(action, true)); resize(240);
  fireEvent.click(screen.getByRole("button", { name: "Add to citation" }));
  fireEvent.click(screen.getByRole("button", { name: "Hide" }));
  expect(action).toHaveBeenCalledTimes(2);
  expect(screen.getByRole("button", { name: "Hide" }).compareDocumentPosition(screen.getByRole("button", { name: "More" })) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Shortcuts" })).toBeNull();
});
