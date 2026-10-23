import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { galleryStates } from "../../gallery-fixtures";
import { PickerFooter } from "./PickerFooter";

const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
const footerStyles = readFileSync(new FileURL("./PickerFooter.css", import.meta.url), "utf8");

const library = galleryStates.find((state) => state.id === "tray-20")!.library;
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

function geometry(initial = 660, trailingMargin = 0, constrainHiddenHint = false) {
  let width = initial; let scale = 1; let copy = 162; let shortcuts = 133.8125; let more = 129.75;
  const observers: { callback: () => void; nodes: Set<Element> }[] = [];
  vi.stubGlobal("ResizeObserver", class {
    nodes = new Set<Element>(); constructor(callback: () => void) { observers.push({ callback, nodes: this.nodes }); }
    observe(node: Element) { this.nodes.add(node); } unobserve(node: Element) { this.nodes.delete(node); } disconnect() { this.nodes.clear(); }
  });
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(function (this: HTMLElement) { return this.matches(".library-bar") ? width : 0; });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    let size = this.matches(".footer-status-size") ? 224
      : this.matches(".shortcuts-hint") ? shortcuts
      : this.getAttribute("aria-label")?.startsWith("Copy") ? copy : this.getAttribute("aria-label") === "More" ? more : 0;
    if (constrainHiddenHint && this.matches(".shortcuts-hint") && this.closest("[data-shortcuts-folded]")) {
      // An absolute optional-only row has no in-flow width. Its child needs
      // intrinsic sizing to escape that constraint when measured while hidden.
      const style = getComputedStyle(this);
      if (style.width !== "max-content" || style.maxWidth !== "none") size = 32;
    }
    return new DOMRect(0, 0, this.hidden || this.style.display === "none" ? 0 : size * scale, 32 * scale);
  });
  const computed = window.getComputedStyle;
  vi.spyOn(window, "getComputedStyle").mockImplementation((node) => {
    if (!node.matches(".library-bar, .enter-hints")) return computed(node);
    const style = document.createElement("div").style; style.columnGap = `${(node.matches(".library-bar") ? 8 : 16) * scale}px`; style.marginInlineEnd = `${node.matches(".enter-hints") ? trailingMargin : 0}px`; return style;
  });
  return { resize(next: number, values: { scale?: number; copy?: number; shortcuts?: number; more?: number } = {}) {
    width = next; scale = values.scale ?? scale; copy = values.copy ?? copy; shortcuts = values.shortcuts ?? shortcuts; more = values.more ?? more;
    act(() => { for (const observer of observers) if ([...observer.nodes].some((node) => node.matches(".library-bar"))) observer.callback(); });
  } };
}

function footer(label = "Copy 20 as Pandoc") {
  return <main><input aria-label="Search" /><button>Pointer destination</button><PickerFooter library={library} groupName={String}
    hints={{ enter: label, shiftEnter: "Add to citation", escape: "Hide" }} altHeld={false}
    actions={{ copy: () => undefined, add: () => undefined, escape: () => undefined, keys: () => undefined }}
    more={<button aria-label="More">More</button>} /></main>;
}

test("crowded GNOME controls drop Shortcuts before shortening status, then restore on growth", () => {
  const fit = geometry(); render(footer());
  expect(screen.queryByRole("button", { name: "Shortcuts" })).toBeNull();
  expect(document.querySelector(".library-bar")?.hasAttribute("data-status-compact")).toBe(false);
  for (let i = 0; i < 3; i++) fit.resize(660);
  expect(screen.queryByRole("button", { name: "Shortcuts" })).toBeNull();
  fit.resize(900);
  expect(screen.getByRole("button", { name: "Shortcuts" })).toBeTruthy();
  fit.resize(520);
  expect(screen.queryByRole("button", { name: "Shortcuts" })).toBeNull();
  expect(document.querySelector(".library-bar")?.hasAttribute("data-status-compact")).toBe(true);
  expect(screen.getByRole("button", { name: "Copy 20 as Pandoc" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "More" })).toBeTruthy();
});

test("changing count, format and key-cap widths recomputes fit without losing full status or actions", () => {
  const fit = geometry(900); const view = render(footer());
  const detail = document.querySelector(".library-status")?.getAttribute("title");
  fit.resize(650, { copy: 300, shortcuts: 150, more: 150 });
  view.rerender(footer("Copy 1000 as Chicago"));
  expect(screen.queryByRole("button", { name: "Shortcuts" })).toBeNull();
  expect(document.querySelector(".library-bar")?.hasAttribute("data-status-compact")).toBe(true);
  expect(document.querySelector(".library-status")?.getAttribute("title")).toBe(detail);
  expect(screen.getByRole("button", { name: "Copy 1000 as Chicago" }).getAttribute("aria-keyshortcuts")).toBe("Enter");
  fit.resize(692, { copy: 162, shortcuts: 75, more: 85 }); view.rerender(footer());
  expect(screen.getByRole("button", { name: "Shortcuts" })).toBeTruthy();
  expect(document.querySelector(".library-bar")?.hasAttribute("data-status-compact")).toBe(false);
});

test.each(["Shortcuts", "Search", "Pointer destination"])("folding hands off only owned Shortcuts focus (%s)", (owner) => {
  const fit = geometry(900); render(footer());
  const focused = owner === "Search" ? screen.getByRole("textbox", { name: owner }) : screen.getByRole("button", { name: owner });
  act(() => focused.focus()); fit.resize(660);
  expect(document.activeElement).toBe(owner === "Shortcuts" ? screen.getByRole("button", { name: "More" }) : focused);
  fit.resize(900);
  expect(document.activeElement).toBe(owner === "Shortcuts" ? screen.getByRole("button", { name: "More" }) : focused);
});

test("RTL 200% pressure keeps core actions available and lets them wrap after optional content yields", () => {
  const fit = geometry(); document.documentElement.dir = "rtl";
  try {
    render(footer()); fit.resize(332, { scale: 2 });
    expect(screen.queryByRole("button", { name: "Shortcuts" })).toBeNull();
    expect(screen.getByRole("button", { name: "Copy 20 as Pandoc" })).toBeTruthy();
    const more = screen.getByRole("button", { name: "More" }); const clicked = vi.fn(); more.addEventListener("click", clicked); fireEvent.click(more);
    expect(clicked).toHaveBeenCalledOnce();
  } finally { document.documentElement.dir = ""; }
});

test("platform key-cap changes restore optional content using the new live widths", () => {
  const platform = vi.spyOn(window.navigator, "platform", "get").mockReturnValue("Linux x86_64");
  const fit = geometry(); const view = render(footer());
  expect(document.querySelectorAll(".shortcuts-hint kbd")).toHaveLength(2);
  expect(screen.queryByRole("button", { name: "Shortcuts" })).toBeNull();
  platform.mockReturnValue("MacIntel"); fit.resize(692, { shortcuts: 75, more: 85 }); view.rerender(footer());
  expect(document.querySelectorAll(".shortcuts-hint kbd")).toHaveLength(1);
  expect(screen.getByRole("button", { name: "Shortcuts" })).toBeTruthy();
});


test("the live hint margin restores Shortcuts at the actual fit boundary", () => {
  geometry(678, -4); render(footer());
  expect(screen.getByRole("button", { name: "Shortcuts" })).toBeTruthy();
});

test("an optional-only hint keeps its intrinsic width while folded and restores on growth", () => {
  const style = document.createElement("style");
  style.textContent = ".footer-action { max-width: 100%; }\n" + footerStyles;
  document.head.append(style);
  try {
    const fit = geometry(388, 0, true);
    render(<main><PickerFooter library={null} groupName={String} hints={{ enter: null, shiftEnter: null, escape: "Hide" }}
      altHeld={false} more={<button aria-label="More">More</button>}
      actions={{ copy: () => undefined, add: () => undefined, escape: () => undefined, keys: () => undefined }} /></main>);
    const states = [];
    for (let i = 0; i < 10; i++) {
      fit.resize(388, { shortcuts: 92.078125, more: 85 });
      states.push(document.querySelector(".library-bar")!.hasAttribute("data-shortcuts-folded"));
    }
    expect(states).toEqual(Array(10).fill(true));
    fit.resize(900);
    const shortcuts = screen.getByRole("button", { name: "Shortcuts" });
    act(() => shortcuts.focus());
    fit.resize(388);
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "More" }));
  } finally { style.remove(); }
});
