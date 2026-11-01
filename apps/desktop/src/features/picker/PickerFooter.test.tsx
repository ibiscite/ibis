import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { PickerFooter } from "./PickerFooter";

const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
const footerStyles = readFileSync(new FileURL("./PickerFooter.css", import.meta.url), "utf8");
const buttonStyles = readFileSync(new FileURL("../../ui/Button.css", import.meta.url), "utf8");
const pickerStyles = readFileSync(new FileURL("../../styles.css", import.meta.url), "utf8");

afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); document.documentElement.dir = ""; });

function geometry(initialWidth = 720) {
  let available = initialWidth;
  let sizes = { copy: 180, more: 100, scale: 1 };
  const callbacks: (() => void)[] = [];
  vi.stubGlobal("ResizeObserver", class { constructor(callback: () => void) { callbacks.push(callback); } observe() {} disconnect() {} });
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(function (this: HTMLElement) { return this.matches(".library-bar") ? available : 0; });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const size = this.matches(".footer-status-size") ? 224 : this.matches(".footer-status-minimum") ? 44
      : this.matches("button[aria-label='More']") ? sizes.more : this.matches("button") ? sizes.copy : 0;
    return new DOMRect(0, 0, size * sizes.scale, 24 * sizes.scale);
  });
  return (width: number, change: Partial<typeof sizes> = {}) => {
    available = width; sizes = { ...sizes, ...change };
    act(() => callbacks.forEach((callback) => callback()));
  };
}

function footer(copy = vi.fn(), altHeld = false, label = "Copy 20 as Pandoc") {
  return <PickerFooter library={null} groupName={String} hints={{ enter: label, shiftEnter: "Add to citation", escape: "Hide" }}
    altHeld={altHeld} actions={{ copy, add: copy, escape: copy }} more={<button aria-label="More">More</button>} />;
}

test("the short enlarged layout drops only hints and keeps status and More reachable, then restores actions", () => {
  const style = document.createElement("style");
  // jsdom does not evaluate container queries. Apply the actual narrow rule
  // to the real footer DOM; the probe checks when it applies and its boxes.
  const short = pickerStyles.slice(pickerStyles.indexOf("@media (max-height: 400px)"));
  const hints = short.match(/\.enter-hints\s*\{([^}]+)\}/)?.[1];
  expect(hints).toBeTruthy();
  render(footer());
  const copy = screen.getByRole("button", { name: "Copy 20 as Pandoc" });
  const status = document.querySelector(".library-status")!.textContent;
  style.textContent = `.enter-hints { ${hints} }`;
  document.head.append(style);
  try {
    expect(screen.queryByRole("button", { name: "Copy 20 as Pandoc" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Shortcuts" })).toBeNull();
    const more = screen.getByRole("button", { name: "More" });
    act(() => more.focus());
    expect(document.activeElement).toBe(more);
    expect(document.querySelector(".library-status")!.textContent).toBe(status);
  } finally { style.remove(); }
  expect(screen.getByRole("button", { name: "Copy 20 as Pandoc" })).toBe(copy);
  expect(copy.querySelector(".key-hint")?.hasAttribute("data-primary")).toBe(true);
  expect(copy.getAttribute("aria-keyshortcuts")).toBe("Enter");
});

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

test("changing count, format and key-cap widths recomputes fit without losing full status or actions", () => {
  const resize = geometry(900); const view = render(footer());
  const status = document.querySelector(".library-status")!;
  const detail = status.getAttribute("title");
  resize(650, { copy: 480, more: 150 });
  view.rerender(footer(vi.fn(), false, "Copy 1000 as Chicago"));
  const bar = document.querySelector(".library-bar")!;
  expect(bar.hasAttribute("data-status-compact")).toBe(true);
  expect(bar.hasAttribute("data-hints-compact")).toBe(true);
  expect(status.getAttribute("title")).toBe(detail);
  expect(screen.getByRole("button", { name: "Copy 1000 as Chicago" }).getAttribute("aria-keyshortcuts")).toBe("Enter");
  expect(screen.getByRole("button", { name: "More" })).toBeTruthy();
  resize(692, { copy: 162, more: 85 }); view.rerender(footer());
  expect(bar.hasAttribute("data-status-compact")).toBe(false);
  expect(bar.hasAttribute("data-hints-compact")).toBe(false);
  expect(screen.queryByRole("button", { name: "Shortcuts" })).toBeNull();
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
  expect(footerStyles).toMatch(/\[data-hints-compact\][^{]+\.key-hint-label \{[^}]*clip-path: inset\(50%\)/);
  expect(buttonStyles).toMatch(/\.ui-button > \.key-hint \{[^}]*flex-wrap: wrap/);
  expect(screen.queryByRole("button", { name: "Shortcuts" })).toBeNull();
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
