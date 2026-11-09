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

test("changing count, format and key-cap widths recomputes fit without losing full status or actions", () => {
  const resize = geometry(900); const view = render(footer());
  const status = document.querySelector(".library-status")!;
  const detail = status.getAttribute("title");
  resize(650, { copy: 480, more: 150 });
  view.rerender(footer(vi.fn(), false, "Copy 1000 as Chicago"));
  const bar = document.querySelector(".library-bar")!;
  expect(bar.hasAttribute("data-status-compact")).toBe(true);
  // A long label truncates; only the window's width folds labels to keys.
  expect(bar.hasAttribute("data-hints-compact")).toBe(false);
  expect(status.getAttribute("title")).toBe(detail);
  expect(screen.getByRole("button", { name: "Copy 1000 as Chicago" }).getAttribute("aria-keyshortcuts")).toBe("Enter");
  expect(screen.getByRole("button", { name: "More" })).toBeTruthy();
  resize(692, { copy: 162, more: 85 }); view.rerender(footer());
  expect(bar.hasAttribute("data-status-compact")).toBe(false);
  expect(bar.hasAttribute("data-hints-compact")).toBe(false);
  expect(screen.queryByRole("button", { name: "Shortcuts" })).toBeNull();
});

test.each([
  ["a short action", "Copy IEEE entry"],
  ["a count", "Copy 2 IEEE entries"],
  ["a long destination", "Paste into Claude · Chicago Manual of Style entry"],
  ["no action", null],
])("More keeps its place in the same element with %s", (_, label) => {
  const resize = geometry(420);
  const view = render(footer());
  const bar = document.querySelector(".library-bar")!;
  const more = screen.getByRole("button", { name: "More" });
  const slot = more.closest(".more")!;
  view.rerender(footer(vi.fn(), false, label));
  // A label of any length can never fold More's own label and resize it.
  resize(420, { copy: label ? 40 + label.length * 8 : 0 });
  expect(screen.getByRole("button", { name: "More" })).toBe(more);
  expect(bar.lastElementChild).toBe(slot);
  expect(bar.hasAttribute("data-hints-compact")).toBe(false);
  view.rerender(footer(vi.fn(), true, label));
  expect(screen.getByRole("button", { name: "More" })).toBe(more);
  expect(bar.lastElementChild).toBe(slot);
});

test("a truncated action still shortens the status first, by the width its name wants", () => {
  const resize = geometry(600); render(footer());
  const bar = document.querySelector(".library-bar")!;
  const label = screen.getByRole("button", { name: "Copy 20 as Pandoc" }).querySelector(".key-hint-label")!;
  Object.defineProperty(label, "scrollWidth", { value: 400 });
  Object.defineProperty(label, "clientWidth", { value: 100 });
  resize(600);
  expect(bar.hasAttribute("data-status-compact")).toBe(true);
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

test("a recovery action makes status compact before Copy folds to a key", () => {
  const resize = geometry(520);
  const withStatus = (hasFix: boolean) => <PickerFooter library={null} groupName={String}
    hints={{ enter: "Copy 20 as Pandoc", shiftEnter: null, escape: "Hide" }} altHeld={false}
    actions={{ copy: vi.fn(), add: vi.fn(), escape: vi.fn() }} more={<button aria-label="More">More</button>}
    status={<span className="library-status-controls"><span className="library-status">Status</span>
      {hasFix && <button className="source-action">Find invented-thesis.bib</button>}</span>} />;
  const view = render(withStatus(false));
  const bar = document.querySelector(".library-bar")!;
  expect(bar.hasAttribute("data-status-compact")).toBe(false);
  view.rerender(withStatus(true));
  resize(520);
  expect(bar.hasAttribute("data-status-compact")).toBe(true);
  expect(bar.hasAttribute("data-hints-compact")).toBe(false);
  expect(screen.getByRole("button", { name: "Find invented-thesis.bib" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Copy 20 as Pandoc" })).toBeTruthy();
});

test.each(["More", "Copy 20 as Pandoc", "Search", "Pointer destination"])("folding preserves the current control's focus (%s)", (owner) => {
  const resize = geometry(900);
  render(<><input aria-label="Search" /><button>Pointer destination</button>{footer()}</>);
  const focused = owner === "Search" ? screen.getByRole("textbox", { name: owner }) : screen.getByRole("button", { name: owner });
  act(() => focused.focus()); resize(300);
  expect(document.activeElement).toBe(focused);
  resize(900);
  expect(document.activeElement).toBe(focused);
});

test("RTL at 200% shortens status and hides only labels while core actions keep their names and targets", () => {
  const resize = geometry(); document.documentElement.dir = "rtl";
  render(footer()); resize(332, { scale: 2 });
  const bar = document.querySelector(".library-bar")!;
  expect(bar.hasAttribute("data-status-compact")).toBe(true);
  expect(bar.hasAttribute("data-hints-compact")).toBe(true);
  expect(screen.getByRole("button", { name: "More" })).toBeTruthy();
  const copy = screen.getByRole("button", { name: "Copy 20 as Pandoc" });
  expect(copy.getAttribute("aria-keyshortcuts")).toBe("Enter");
  expect(bar.textContent).toContain("Library details unavailable");
  expect(screen.queryByRole("button", { name: "Shortcuts" })).toBeNull();
});

test("RTL and Alt preserve the guarded Add and Escape actions without a shortcut hint", () => {
  const resize = geometry(); document.documentElement.dir = "rtl";
  const action = vi.fn(); render(footer(action, true)); resize(240);
  fireEvent.click(screen.getByRole("button", { name: "Add to Citation" }));
  fireEvent.click(screen.getByRole("button", { name: "Hide" }));
  expect(action).toHaveBeenCalledTimes(2);
  expect(screen.getByRole("button", { name: "Hide" }).compareDocumentPosition(screen.getByRole("button", { name: "More" })) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Shortcuts" })).toBeNull();
});
