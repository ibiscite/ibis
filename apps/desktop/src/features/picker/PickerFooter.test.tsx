import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { PickerFooter } from "./PickerFooter";

afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); document.documentElement.dir = ""; });

function geometry(initialWidth = 720) {
  let available = initialWidth;
  let sizes = { copy: 180, more: 100, scale: 1 };
  const callbacks: (() => void)[] = [];
  vi.stubGlobal("ResizeObserver", class { constructor(callback: () => void) { callbacks.push(callback); } observe() {} disconnect() {} });
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(function (this: HTMLElement) { return this.matches(".library-bar") ? available : 0; });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const size = this.matches(".footer-status-size") ? 224 : this.matches(".footer-status-minimum") ? 44
      : this.matches(".footer-label-minimum") ? 120
      : this.matches("button[aria-label='More']") ? sizes.more : this.matches("button") ? sizes.copy : 0;
    return new DOMRect(0, 0, size * sizes.scale, 24 * sizes.scale);
  });
  return (width: number, change: Partial<typeof sizes> = {}) => {
    available = width; sizes = { ...sizes, ...change };
    act(() => callbacks.forEach((callback) => callback()));
  };
}

function footer(copy = vi.fn(), altHeld = false, label: string | null = "Copy 20 as Pandoc") {
  return <PickerFooter library={null} groupName={String} hints={{ enter: label, shiftEnter: "Add to Citation", escape: "Hide" }}
    altHeld={altHeld} actions={{ copy, add: copy, escape: copy }} more={<button aria-label="More">More</button>} />;
}

test("the footer is status, Copy, More with no Shortcuts button at any width", () => {
  const resize = geometry(); render(footer());
  const bar = document.querySelector(".library-bar")!;
  const more = screen.getByRole("button", { name: "More" });
  const copy = screen.getByRole("button", { name: "Copy 20 as Pandoc" });
  expect(copy.compareDocumentPosition(more) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  // The launcher action: quiet words, the accent on its ↵ cap (docs/design/picker.md, Quiet bands).
  expect(copy.getAttribute("data-variant")).toBe("action");
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
  resize(260);
  expect(bar.hasAttribute("data-hints-compact")).toBe(true);
  expect(action.getAttribute("aria-keyshortcuts")).toBe("Enter");
  expect(document.activeElement).toBe(action);
  fireEvent.click(action);
  expect(copy).toHaveBeenCalledOnce();
  resize(900);
  expect(bar.hasAttribute("data-status-compact")).toBe(false);
  expect(bar.hasAttribute("data-hints-compact")).toBe(false);
});

test("holding Alt folds the action labels to keys before other labels", () => {
  const resize = geometry(500);
  const view = render(footer());
  const bar = document.querySelector(".library-bar")!;
  resize(500);
  expect(bar.hasAttribute("data-alt-compact")).toBe(false);
  view.rerender(footer(vi.fn(), true));
  resize(500);
  expect(bar.hasAttribute("data-alt-compact")).toBe(true);
  expect(bar.hasAttribute("data-hints-compact")).toBe(false);
  view.rerender(footer());
  expect(bar.hasAttribute("data-alt-compact")).toBe(false);
});
