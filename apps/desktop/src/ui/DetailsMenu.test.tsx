import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { useRef, useState } from "react";
import type { SearchResult } from "../api";
import { CitationShelf } from "../features/picker/CitationShelf";
import { DetailsMenu } from "./DetailsMenu";

afterEach(async () => {
  cleanup();
  await act(async () => { await new Promise(requestAnimationFrame); });
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

function nativeLayout() {
  const observers: { nodes: Set<Element>; callback: ResizeObserverCallback; observer: ResizeObserver }[] = [];
  vi.stubGlobal("ResizeObserver", class implements ResizeObserver {
    nodes = new Set<Element>();
    constructor(callback: ResizeObserverCallback) { observers.push({ nodes: this.nodes, callback, observer: this }); }
    observe(node: Element) { this.nodes.add(node); }
    unobserve(node: Element) { this.nodes.delete(node); }
    disconnect() { this.nodes.clear(); }
  });
  let anchorTop = 320, anchorHeight = 64;
  const frameHeight = (menu: HTMLElement) => Math.min(480, parseFloat(menu.style.maxHeight) || 480);
  const frame = () => {
    const menu = document.querySelector<HTMLElement>(".details-menu");
    if (!menu) return new DOMRect();
    const height = frameHeight(menu);
    return new DOMRect(parseFloat(menu.style.left) || 16, menu.style.bottom ? innerHeight - parseFloat(menu.style.bottom) - height : parseFloat(menu.style.top) || 0, 448, height);
  };
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.matches(".shelf-output, .shelf-segment-button, [data-segment]")) return new DOMRect(16, anchorTop, 448, anchorHeight);
    if (this.matches(".search-bar")) return new DOMRect(0, 0, innerWidth, 185);
    if (this.matches(".picker-content")) return new DOMRect(0, 185, innerWidth, Math.max(0, anchorTop - 185));
    if (this.matches("[data-anchor-port]")) return new DOMRect(0, 185, innerWidth, 80);
    if (this.matches(".details-menu")) return frame();
    if (this.matches(".details-menu-scroll")) { const rect = frame(); return new DOMRect(17, rect.top + 87, 446, Math.max(0, rect.height - 94)); }
    if (this.matches(".details-menu input, .details-menu button")) return new DOMRect(33, frame().top + 100, 200, 64);
    return new DOMRect(0, 0, innerWidth, innerHeight);
  });
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) { return this.matches(".details-menu") ? frameHeight(this) : innerHeight; });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockImplementation(function (this: HTMLElement) { return this.matches(".details-menu") ? 448 : innerWidth; });
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockImplementation(function (this: HTMLElement) { return this.matches(".details-menu-scroll") ? Math.max(0, frame().height - 94) : innerHeight; });
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(() => innerWidth);
  vi.spyOn(HTMLElement.prototype, "scrollHeight", "get").mockImplementation(function (this: HTMLElement) { return this.matches(".details-menu-scroll") ? 386 : 0; });
  vi.spyOn(HTMLElement.prototype, "offsetParent", "get").mockReturnValue(document.documentElement);
  const computed = window.getComputedStyle;
  vi.spyOn(window, "getComputedStyle").mockImplementation((node) => {
    const style = computed(node);
    if (node === document.documentElement) Object.defineProperty(style, "fontSize", { value: "32px" });
    return style;
  });
  return {
    move(top: number, height = 64) { anchorTop = top; anchorHeight = height; },
    delayed(node: Element) {
      const pending = observers.filter(({ nodes }) => nodes.has(node));
      return () => { for (const { callback, observer } of pending) callback([{ target: node } as ResizeObserverEntry], observer); };
    },
    async resize(node: Element) {
      await act(async () => {
        for (const { nodes, callback, observer } of observers) if (nodes.has(node)) callback([{ target: node } as ResizeObserverEntry], observer);
        await new Promise(requestAnimationFrame);
      });
    },
  };
}

test("200% text refits after the middle settles even when the shelf stays folded", async () => {
  vi.stubGlobal("innerWidth", 720); vi.stubGlobal("innerHeight", 480);
  const layout = nativeLayout();
  function Picker() {
    const [reference, setReference] = useState<SearchResult>({
      revision: "1", id: { source: "refs.bib", native_key: "invented" }, key: "invented", title: "Invented reference", authors: "Aster",
      year: 2026, matched: ["title"], partial: false, first_author: "Aster", author_count: 1, container: null, item_type: "book",
    });
    return <main><header className="search-bar" /><div className="picker-content" />
      <CitationShelf selected={new Map()} highlightedReference={reference} announce={false} removeLabel={() => "Remove reference"} onRemove={() => undefined}
        onChangeReference={(previous, update) => setReference({ ...previous, ...update })} style={null} keys focusable={false}
        shown={{ text: "[@invented]", markup: [], html: "", warnings: [] }} outputWarnings={[]} /><footer /></main>;
  }
  render(<Picker />);
  expect(document.querySelector(".shelf")!.getAttribute("data-folded")).toBe("true");
  fireEvent.click(screen.getByRole("button", { name: /^Edit / }));
  const page = screen.getByRole<HTMLInputElement>("textbox", { name: "Page" });
  act(() => page.focus()); fireEvent.change(page, { target: { value: "12" } });
  vi.stubGlobal("innerWidth", 480); vi.stubGlobal("innerHeight", 360);
  layout.move(185); fireEvent.resize(window);
  layout.move(200);
  await layout.resize(document.querySelector(".picker-content")!);
  const menu = document.querySelector<HTMLElement>(".details-menu")!;
  const bounds = menu.getBoundingClientRect();
  expect(bounds.bottom).toBe(194);
  expect(bounds.top).toBeGreaterThanOrEqual(8);
  expect(bounds.left).toBeGreaterThanOrEqual(8);
  expect(bounds.right).toBeLessThanOrEqual(472);
  expect(parseFloat(menu.style.maxHeight)).toBe(186);
  const scroll = menu.querySelector<HTMLElement>(".details-menu-scroll")!;
  expect(scroll.scrollHeight).toBeGreaterThan(scroll.clientHeight);
  expect(scroll.clientHeight).toBeGreaterThanOrEqual(page.getBoundingClientRect().height);
  expect(screen.getByRole("textbox", { name: "Page" })).toBe(page);
  expect(page.value).toBe("12"); expect(document.activeElement).toBe(page);
  // The later probe steps scroll long titles, refusals, warnings and choices
  // inside this port. That scroll must not dismiss or refocus the editor.
  await act(async () => { scroll.scrollTop = 200; fireEvent.scroll(scroll); await new Promise(requestAnimationFrame); });
  expect(document.querySelector(".details-menu")).toBe(menu);
  expect(menu.getBoundingClientRect().bottom).toBe(194);
  expect(scroll.scrollTop).toBe(200);
  expect(page.value).toBe("12"); expect(document.activeElement).toBe(page);
});

test.each(["off window", "scrolled out", "no room", "no usable editor"])("an editor with %s dismisses without losing edits or scrolling its segment back", async (reason) => {
  vi.stubGlobal("innerWidth", 480); vi.stubGlobal("innerHeight", 360);
  const layout = nativeLayout(); layout.move(200);
  function Picker() {
    const anchor = useRef<HTMLButtonElement>(null), search = useRef<HTMLInputElement>(null);
    const [open, setOpen] = useState(false), [page, setPage] = useState("");
    return <main><input ref={search} aria-label="Search" /><div data-anchor-port style={{ overflowY: "auto" }}><button data-segment ref={anchor} type="button" onClick={() => setOpen(true)}>Reference segment</button></div>
      {open && anchor.current && <DetailsMenu anchor={anchor.current} returnFocusRef={search} title="Citation details" onClose={() => setOpen(false)}>
        <input aria-label="Page" value={page} onChange={(event) => setPage(event.target.value)} />
      </DetailsMenu>}
    </main>;
  }
  render(<Picker />);
  fireEvent.click(screen.getByRole("button", { name: "Reference segment" }));
  const page = screen.getByRole<HTMLInputElement>("textbox", { name: "Page" });
  act(() => page.focus()); fireEvent.change(page, { target: { value: "12" } });
  if (reason === "off window") layout.move(-80);
  else if (reason === "scrolled out") layout.move(80);
  else if (reason === "no room") layout.move(8, 344);
  else layout.move(120, 120);
  await layout.resize(document.querySelector("main")!);
  await act(async () => { await new Promise(requestAnimationFrame); });
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(document.activeElement).toBe(screen.getByRole("textbox", { name: "Search" }));
  expect(document.querySelector("main")!.scrollTop).toBe(0);
  layout.move(200);
  fireEvent.click(screen.getByRole("button", { name: "Reference segment" }));
  expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Page" }).value).toBe("12");
});

test("a delayed layout event from a dismissed segment cannot close its replacement's editor", async () => {
  vi.stubGlobal("innerWidth", 480); vi.stubGlobal("innerHeight", 360);
  const layout = nativeLayout(); layout.move(200);
  function Picker({ source }: { source: "A" | "B" }) {
    const [anchor, setAnchor] = useState<HTMLElement | null>(null), [page, setPage] = useState("");
    return <main>{(["A", "B"] as const).map((name) => <button key={name} hidden={source !== name} data-segment type="button" onClick={(event) => setAnchor(event.currentTarget)}>Reference {name}</button>)}
      {anchor && <DetailsMenu anchor={anchor} title="Citation details" onClose={() => setAnchor(null)}>
        <input aria-label="Page" value={page} onChange={(event) => setPage(event.target.value)} />
      </DetailsMenu>}
    </main>;
  }
  const { rerender } = render(<Picker source="A" />);
  fireEvent.click(screen.getByRole("button", { name: "Reference A" }));
  const old = screen.getByRole("textbox", { name: "Page" });
  act(() => old.focus()); fireEvent.change(old, { target: { value: "12" } });
  const late = layout.delayed(document.querySelector("main")!);
  fireEvent.keyDown(old, { key: "Escape" });
  rerender(<Picker source="B" />);
  fireEvent.click(screen.getByRole("button", { name: "Reference B" }));
  const current = screen.getByRole<HTMLInputElement>("textbox", { name: "Page" });
  act(() => current.focus());
  await act(async () => { late(); await new Promise(requestAnimationFrame); });
  expect(screen.getByRole("textbox", { name: "Page" })).toBe(current);
  expect(current.value).toBe("12"); expect(document.activeElement).toBe(current);
});

test.each([192, 700])("folding the shelf keeps its %spx editor inside the window and attached to the same citation segment", async (contentHeight) => {
  vi.stubGlobal("innerWidth", 720);
  vi.stubGlobal("innerHeight", 480);
  const height = (menu: HTMLElement) => Math.min(contentHeight, parseFloat(menu.style.maxHeight) || contentHeight);
  // jsdom has no layout engine. Supply the native geometry changes in their
  // real order: viewport resize, then a shelf fold that moves (but does not
  // resize) the same citation segment. React Aria still owns the fitting.
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.matches(".shelf-output, .shelf-segment-button")) return new DOMRect(8, this.closest("[data-folded]") ? 272 : 249, 464, 40);
    if (this.matches(".details-menu")) {
      const top = this.style.bottom ? innerHeight - parseFloat(this.style.bottom) - height(this) : parseFloat(this.style.top) || 0;
      return new DOMRect(parseFloat(this.style.left) || 8, top, 464, height(this));
    }
    return new DOMRect(0, 0, innerWidth, innerHeight);
  });
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) { return this.matches(".details-menu") ? height(this) : innerHeight; });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockImplementation(function (this: HTMLElement) { return this.matches(".details-menu") ? 464 : innerWidth; });
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockImplementation(() => innerHeight);
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(() => innerWidth);
  vi.spyOn(HTMLElement.prototype, "offsetParent", "get").mockReturnValue(document.documentElement);
  function Picker() {
    const [reference, setReference] = useState<SearchResult>({
      revision: "1", id: { source: "refs.bib", native_key: "invented" }, key: "invented", title: "Invented reference", authors: "Aster",
      year: 2026, matched: ["title"], partial: false, first_author: "Aster", author_count: 1, container: null, item_type: "book",
    });
    return <CitationShelf selected={new Map()} highlightedReference={reference} announce={false} removeLabel={() => "Remove reference"} onRemove={() => undefined}
      onChangeReference={(previous, update) => setReference({ ...previous, ...update })} style={null} keys focusable={false}
      shown={{ text: "[@invented]", markup: [], html: "", warnings: [] }} outputWarnings={[]} />;
  }
  render(<Picker />);
  fireEvent.click(screen.getByRole("button", { name: /^Edit / }));
  const page = screen.getByRole<HTMLInputElement>("textbox", { name: "Page" });
  act(() => page.focus());
  fireEvent.change(page, { target: { value: "12" } });
  const menu = document.querySelector<HTMLElement>(".details-menu")!;
  expect(menu.getBoundingClientRect().bottom).toBe(243);
  vi.stubGlobal("innerWidth", 480);
  vi.stubGlobal("innerHeight", 360);
  fireEvent.resize(window);
  await act(async () => { await new Promise(requestAnimationFrame); });
  const bounds = menu.getBoundingClientRect();
  expect(bounds.bottom).toBe(266);
  expect(bounds.top).toBeGreaterThanOrEqual(8);
  expect(bounds.right).toBeLessThanOrEqual(472);
  expect(parseFloat(menu.style.maxHeight)).toBeLessThanOrEqual(258);
  expect(page.value).toBe("12");
  expect(document.activeElement).toBe(page);
  vi.stubGlobal("innerWidth", 720);
  vi.stubGlobal("innerHeight", 480);
  fireEvent.resize(window);
  await act(async () => { await new Promise(requestAnimationFrame); });
  expect(menu.getBoundingClientRect().bottom).toBe(243);
  expect(page.value).toBe("12");
  expect(document.activeElement).toBe(page);
  fireEvent.keyDown(page, { key: "Escape" });
  expect(document.querySelector(".details-menu")).toBeNull();
  vi.stubGlobal("innerWidth", 480);
  vi.stubGlobal("innerHeight", 360);
  fireEvent.resize(window);
  fireEvent.click(screen.getByRole("button", { name: /^Edit / }));
  expect(document.querySelector(".details-menu")!.getBoundingClientRect().bottom).toBe(266);
  expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Page" }).value).toBe("12");
});

test.each([false, true])("Tab closes reference details and moves past its trigger (backward=%s)", async (backward) => {
  const pickerKey = vi.fn();
  function Picker() {
    const anchor = useRef<HTMLButtonElement>(null);
    const [open, setOpen] = useState(false);
    return <main onKeyDown={pickerKey}><button type="button">Before</button><button ref={anchor} type="button" onClick={() => setOpen(true)}>Reference segment</button><button type="button">After</button>
      {open && anchor.current && <DetailsMenu anchor={anchor.current} title="Citation details" onClose={() => setOpen(false)}>
        <input aria-label="Page" /><button type="button">Last action</button>
      </DetailsMenu>}
    </main>;
  }
  render(<Picker />);
  const trigger = screen.getByRole("button", { name: "Reference segment" });
  act(() => trigger.focus()); fireEvent.click(trigger);
  const close = screen.getByRole("dialog");
  act(() => close.focus());
  for (const composition of [{ isComposing: true }, { keyCode: 229 }]) {
    fireEvent.keyDown(close, { key: "Tab", shiftKey: backward, ...composition });
    expect(document.activeElement).toBe(close);
  }
  pickerKey.mockClear();
  for (const key of ["k", "i", "/", "Enter"]) fireEvent.keyDown(close, { key, ctrlKey: true });
  expect(pickerKey).not.toHaveBeenCalled();
  fireEvent.keyDown(close, { key: "Tab", shiftKey: backward });
  await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("button", { name: backward ? "Before" : "After" })));
  expect(screen.queryByRole("dialog")).toBeNull();
});

test("folding the originating segment preserves the edit and Escape returns to the visible shelf control", async () => {
  function Picker({ folded }: { folded: boolean }) {
    const origin = useRef<HTMLButtonElement>(null), count = useRef<HTMLButtonElement>(null);
    const [open, setOpen] = useState(false);
    const anchor = folded ? count.current : origin.current;
    return <><button ref={origin} hidden={folded} type="button" onClick={() => setOpen(true)}>Reference segment</button>
      <button ref={count} hidden={!folded} type="button" onClick={() => setOpen(true)}>Selected references</button>
      {open && anchor && <DetailsMenu anchor={anchor} title="Citation details" onClose={() => setOpen(false)}><input aria-label="Page" /></DetailsMenu>}
    </>;
  }
  const { rerender } = render(<Picker folded={false} />);
  const origin = screen.getByRole("button", { name: "Reference segment" });
  act(() => origin.focus());
  fireEvent.click(origin);
  const page = screen.getByRole<HTMLInputElement>("textbox", { name: "Page" });
  act(() => page.focus());
  fireEvent.change(page, { target: { value: "12" } });
  rerender(<Picker folded />);
  expect(document.activeElement).toBe(page);
  expect(page.value).toBe("12");
  fireEvent.keyDown(page, { key: "Escape" });
  await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("button", { name: "Selected references" })));
});
