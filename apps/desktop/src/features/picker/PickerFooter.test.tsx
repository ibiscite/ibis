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
      : this.matches(".footer-label-minimum") ? 120
      : this.matches("button[aria-label='More']") ? sizes.more : this.matches("button") ? sizes.copy : 0;
    return new DOMRect(0, 0, size * sizes.scale, 24 * sizes.scale);
  });
  return (width: number, change: Partial<typeof sizes> = {}) => {
    available = width; sizes = { ...sizes, ...change };
    act(() => callbacks.forEach((callback) => callback()));
  };
}

function rule(selector: string) {
  const at = footerStyles.indexOf(`${selector} {`);
  expect(at).toBeGreaterThanOrEqual(0);
  return footerStyles.slice(at, footerStyles.indexOf("}", at));
}

function footer(copy = vi.fn(), altHeld = false, label: string | null = "Copy 20 as Pandoc") {
  return <PickerFooter library={null} groupName={String} hints={{ enter: label, shiftEnter: "Add to Citation", escape: "Hide" }}
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

test("the footer is status, Copy, More with no Shortcuts button at any width", () => {
  const resize = geometry(); render(footer());
  const bar = document.querySelector(".library-bar")!;
  const more = screen.getByRole("button", { name: "More" });
  const copy = screen.getByRole("button", { name: "Copy 20 as Pandoc" });
  expect(copy.compareDocumentPosition(more) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
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

test("holding Alt keeps the footer one line: Alt's labels fold to keys, More keeps its label", () => {
  const resize = geometry(500);
  const view = render(footer());
  const bar = document.querySelector(".library-bar")!;
  resize(500);
  expect(bar.hasAttribute("data-alt-compact")).toBe(false);
  view.rerender(footer(vi.fn(), true));
  resize(500);
  expect(bar.hasAttribute("data-alt-compact")).toBe(true);
  expect(bar.hasAttribute("data-hints-compact")).toBe(false);
  expect(footerStyles).toMatch(/\.library-bar > \.enter-hints \{ grid-area: 1 \/ 2; flex-wrap: nowrap; \}/);
  expect(footerStyles).toMatch(/\[data-alt-compact\] \.enter-hints \.key-hint-label/);
  expect(footerStyles).toMatch(/\[data-alt-compact\]\[data-hints-compact\] > \.enter-hints \{ overflow: hidden; \}/);
  view.rerender(footer());
  expect(bar.hasAttribute("data-alt-compact")).toBe(false);
});

test("the leading slot reserves its fix's width, and a failure line spans the footer", () => {
  const resize = geometry(520);
  render(<PickerFooter library={null} groupName={String} hints={{ enter: "Copy 20 as Harvard Cite Them Right", shiftEnter: null, escape: "Hide" }}
    altHeld={false} actions={{ copy: vi.fn(), add: vi.fn(), escape: vi.fn() }} more={<button aria-label="More">More</button>}
    status={<span className="library-status-controls"><span className="library-status">Status</span><button className="source-action">Find invented-thesis.bib</button></span>} />);
  resize(520);
  const bar = document.querySelector<HTMLElement>(".library-bar")!;
  expect(bar.style.getPropertyValue("--footer-fix")).toBe("180px");
  expect(rule(".library-bar")).toMatch(/--footer-fix/);
  expect(rule(".library-bar[data-status-compact]")).toMatch(/--footer-fix/);
  expect(footerStyles).toMatch(/\.library-bar > :is\(\.ui-quiet-status, \.library-action-error\) \{ grid-column: 1 \/ -1; \}/);
});

test("the action column truncates its label and can never push More off the trailing column", () => {
  expect(rule(".library-bar")).toMatch(/display: grid/);
  expect(rule(".library-bar")).toMatch(/grid-template-columns: [^;]*minmax\(0, auto\) auto/);
  expect(rule(".library-bar > .more")).toMatch(/grid-area: 1 \/ 3/);
  // The leading slot can hold anything: only the two fixed columns are named.
  expect(footerStyles).toMatch(/\.library-bar > :not\(\.enter-hints, \.more, \.ui-quiet-status, \.library-action-error\) \{ grid-column: 1; min-width: 0; \}/);
  expect(rule(".library-bar .footer-action .key-hint-label")).toMatch(/text-overflow: ellipsis/);
  expect(pickerStyles).not.toMatch(/\.more \{[^}]*margin-inline-start: auto/);
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
  expect(buttonStyles).toMatch(/\.ui-button\[data-variant\] > \.key-hint \{[^}]*flex-wrap: wrap/);
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
