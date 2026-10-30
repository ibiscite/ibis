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

test.each([192, 700])("folding the shelf keeps its %spx editor inside the window and attached to the same citation segment", async (contentHeight) => {
  vi.stubGlobal("innerWidth", 720);
  vi.stubGlobal("innerHeight", 480);
  const height = (menu: HTMLElement) => Math.min(contentHeight, parseFloat(menu.style.maxHeight) || contentHeight);
  // jsdom has no layout engine. Supply the native geometry changes in their
  // real order: viewport resize, then a shelf fold that moves (but does not
  // resize) the same citation segment. React Aria still owns the fitting.
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.matches(".shelf-output")) return new DOMRect(8, this.closest("[data-folded]") ? 272 : 249, 464, 40);
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
  fireEvent.click(screen.getByRole("button", { name: "Edit citation" }));
  const page = screen.getByRole<HTMLInputElement>("textbox", { name: "Page" });
  act(() => page.focus());
  fireEvent.change(page, { target: { value: "12" } });
  const menu = document.querySelector<HTMLElement>(".details-menu")!;
  expect(menu.getBoundingClientRect().bottom).toBe(243);
  vi.stubGlobal("innerWidth", 480);
  vi.stubGlobal("innerHeight", 360);
  fireEvent.resize(window);
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
  expect(menu.getBoundingClientRect().bottom).toBe(243);
  expect(page.value).toBe("12");
  expect(document.activeElement).toBe(page);
  fireEvent.keyDown(page, { key: "Escape" });
  expect(document.querySelector(".details-menu")).toBeNull();
  vi.stubGlobal("innerWidth", 480);
  vi.stubGlobal("innerHeight", 360);
  fireEvent.resize(window);
  fireEvent.click(screen.getByRole("button", { name: "Edit citation" }));
  expect(document.querySelector(".details-menu")!.getBoundingClientRect().bottom).toBe(266);
  expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Page" }).value).toBe("12");
});

test("Tab wraps both ways inside the anchored details menu while picker shortcuts stay blocked", async () => {
  const pickerKey = vi.fn();
  function Picker() {
    const anchor = useRef<HTMLButtonElement>(null);
    const [open, setOpen] = useState(false);
    return <div onKeyDown={pickerKey}><button ref={anchor} type="button" onClick={() => setOpen(true)}>Reference segment</button>
      {open && anchor.current && <DetailsMenu anchor={anchor.current} title="Citation details" onClose={() => setOpen(false)}>
        <input aria-label="Page" /><button type="button">Last action</button>
      </DetailsMenu>}
    </div>;
  }
  render(<Picker />);
  fireEvent.click(screen.getByRole("button", { name: "Reference segment" }));
  const close = screen.getByRole("button", { name: "Close" });
  const last = screen.getByRole("button", { name: "Last action" });
  await act(async () => { close.focus(); });
  for (const composition of [{ isComposing: true }, { keyCode: 229 }]) {
    fireEvent.keyDown(close, { key: "Tab", shiftKey: true, ...composition });
    expect(document.activeElement).toBe(close);
  }
  fireEvent.keyDown(close, { key: "Tab", shiftKey: true });
  await waitFor(() => expect(document.activeElement).toBe(last));
  fireEvent.keyDown(last, { key: "Tab" });
  await waitFor(() => expect(document.activeElement).toBe(close));
  pickerKey.mockClear();
  fireEvent.keyDown(close, { key: "k", ctrlKey: true });
  fireEvent.keyDown(close, { key: "i", ctrlKey: true });
  fireEvent.keyDown(close, { key: "/", ctrlKey: true });
  fireEvent.keyDown(close, { key: "Enter" });
  expect(pickerKey).not.toHaveBeenCalled();
  expect(screen.getByRole("dialog", { name: "Citation details" })).toBeTruthy();
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
