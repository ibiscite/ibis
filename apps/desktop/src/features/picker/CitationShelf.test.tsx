import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { useState } from "react";
import type { SearchResult } from "../../api";
import { chipLabels, CitationShelf } from "./CitationShelf";

const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");

afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

function record(key: string, first_author = "García", year: number | null = 2021): SearchResult {
  return {
    revision: "1", id: { source: "refs.bib", native_key: key }, key, title: `Title ${key}`, authors: first_author,
    year, matched: ["title"], first_author, author_count: 1, container: null, item_type: "book",
  };
}

function chosen(records: SearchResult[]) {
  return new Map(records.map((r) => [r.key, r]));
}

const shelf = (props: Partial<Parameters<typeof CitationShelf>[0]>) => (
  <CitationShelf announce={false} selected={new Map()} removeLabel={(r) => `Remove ${r.key}`} onRemove={() => undefined}
    style={null} keys shown={undefined} focusable={false} {...props} />
);

test("chips read as author and year, and add the key only where two would read the same", () => {
  expect(chipLabels([record("a"), record("b", "Okafor", 2019), record("c", "", null)])).toEqual(["García 2021", "Okafor 2019", "c"]);
  expect(chipLabels([record("garcia2021a"), record("garcia2021b")])).toEqual(["García 2021 · garcia2021a", "García 2021 · garcia2021b"]);
});

test("six chips show, the rest fold into +N, and +N shows every chip so each can be removed", () => {
  const records = Array.from({ length: 9 }, (_, i) => record(`k${i}`, `Author${i}`));
  const onRemove = vi.fn();
  render(shelf({ selected: chosen(records), onRemove }));
  const list = screen.getByRole("list", { name: "Selected references" });
  expect(within(list).getAllByRole("button", { name: /^Remove/ })).toHaveLength(6);
  const more = within(list).getByRole("button", { name: "Show all 9 selected references" });
  expect(more.textContent).toBe("+3");
  expect(more.getAttribute("aria-expanded")).toBe("false");
  fireEvent.click(more);
  expect(within(list).getAllByRole("button", { name: /^Remove/ })).toHaveLength(9);
  fireEvent.click(within(list).getByRole("button", { name: "Remove k8" }));
  expect(onRemove).toHaveBeenCalledWith(records[8]);
  // Each chip names its whole key to the pointer.
  expect(within(list).getByText("Author8 2021").closest("li")?.getAttribute("title")).toBe("k8");
});

test("shelf controls share Button states and their measuring copies stay inaccessible", () => {
  const { container } = render(shelf({ selected: chosen(Array.from({ length: 9 }, (_, i) => record(`k${i}`))) }));
  const buttons = screen.getAllByRole("button");
  expect(buttons).toHaveLength(7);
  for (const button of buttons) expect(button.classList.contains("ui-button")).toBe(true);
  const measure = container.querySelector(".chip-fit-measure")!;
  expect(measure.getAttribute("aria-hidden")).toBe("true");
  expect(measure.hasAttribute("inert")).toBe(true);
  for (const button of measure.querySelectorAll("button")) {
    expect(button.classList.contains("ui-button")).toBe(true);
    expect(button.tabIndex).toBe(-1);
    expect(buttons).not.toContain(button);
  }
});

test.each(["Enter", " "])("%s expands, removes and folds exactly once from the keyboard", (key) => {
  const records = Array.from({ length: 9 }, (_, i) => record(`k${i}`));
  const onRemove = vi.fn();
  render(shelf({ selected: chosen(records), onRemove }));
  const press = (button: HTMLElement) => {
    act(() => button.focus());
    const code = key === " " ? "Space" : "Enter";
    fireEvent.keyDown(button, { key, code });
    fireEvent.keyUp(button, { key, code });
  };
  press(screen.getByRole("button", { name: "Show all 9 selected references" }));
  expect(screen.getAllByRole("button", { name: /^Remove/ })).toHaveLength(9);
  press(screen.getByRole("button", { name: "Remove k8" }));
  expect(onRemove).toHaveBeenCalledExactlyOnceWith(records[8]);
  press(screen.getByRole("button", { name: "Show fewer selected references" }));
  expect(screen.getAllByRole("button", { name: /^Remove/ })).toHaveLength(6);
});

test("50 choices fit narrow and 200% text rows, keeping hidden source identities removable", () => {
  let scale = 1;
  let width = 344;
  const observers: { callback: () => void; disconnect: ReturnType<typeof vi.fn> }[] = [];
  vi.stubGlobal("ResizeObserver", class {
    disconnect = vi.fn();
    observe = vi.fn();
    constructor(callback: () => void) {
      let active = false;
      this.observe = vi.fn(() => { active = true; });
      this.disconnect = vi.fn(() => { active = false; });
      observers.push({ callback: () => { if (active) callback(); }, disconnect: this.disconnect });
    }
  });
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(function (this: HTMLElement) { return this.classList.contains("shelf-chips") ? width : 0; });
  const computed = window.getComputedStyle;
  vi.spyOn(window, "getComputedStyle").mockImplementation((element) => {
    if (!element.matches(".chip, .shelf-chips")) return computed(element);
    const style = document.createElement("div").style;
    style.minWidth = `${80 * scale}px`;
    style.columnGap = `${4 * scale}px`;
    style.paddingInlineStart = style.paddingInlineEnd = `${12 * scale}px`;
    return style;
  });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    return new DOMRect(0, 0, this.matches(".chip-more button") ? 40 * scale : 0, 24 * scale);
  });
  const records = Array.from({ length: 50 }, (_, i) => ({ ...record(`shared${i % 25}`, `Author${i}`),
    id: { source: i < 25 ? "first.bib" : "second.bib", native_key: `entry${i}` } }));
  const onRemove = vi.fn();
  function Picker() {
    const [selected, setSelected] = useState(new Map(records.map((r, i) => [`identity${i}`, r])));
    return shelf({ selected, removeLabel: (r) => `Remove ${r.id.source}/${r.id.native_key}`, onRemove: (r) => {
      onRemove(r); setSelected((previous) => new Map([...previous].filter(([, value]) => value !== r)));
    } });
  }
  const view = render(<Picker />);
  const list = screen.getByRole("list", { name: "Selected references" });
  const removeButtons = () => within(list).getAllByRole("button", { name: /^Remove/ });
  expect(removeButtons()).toHaveLength(3);
  expect(within(list).getByRole("button", { name: "Show all 50 selected references" }).textContent).toBe("+47");
  scale = 2; width = 328;
  act(() => { for (const observer of [...observers]) observer.callback(); });
  expect(removeButtons()).toHaveLength(1);
  const more = within(list).getByRole("button", { name: "Show all 50 selected references" });
  expect(more.textContent).toBe("+49");
  fireEvent.click(more);
  expect(removeButtons().map((button) => button.getAttribute("aria-label"))).toEqual(records.map((r) => `Remove ${r.id.source}/${r.id.native_key}`));
  fireEvent.click(within(list).getByRole("button", { name: "Remove second.bib/entry49" }));
  expect(onRemove).toHaveBeenCalledWith(records[49]);
  expect(removeButtons()).toHaveLength(49);
  expect(within(list).getByRole("button", { name: "Remove first.bib/entry24" })).toBeTruthy();
  fireEvent.click(within(list).getByRole("button", { name: "Show fewer selected references" }));
  expect(within(list).getByRole("button", { name: "Show all 49 selected references" }).textContent).toBe("+48");
  scale = 1; width = 1000;
  act(() => { for (const observer of [...observers]) observer.callback(); });
  expect(removeButtons()).toHaveLength(6);
  expect(within(list).getByRole("button", { name: "Show all 49 selected references" }).textContent).toBe("+43");
  removeButtons()[5]!.focus();
  width = 344;
  act(() => { for (const observer of [...observers]) observer.callback(); });
  expect(removeButtons()).toHaveLength(3);
  expect(document.activeElement).toBe(within(list).getByRole("button", { name: "Show all 49 selected references" }));
  width = 1000;
  act(() => { for (const observer of [...observers]) observer.callback(); });
  fireEvent.click(within(list).getByRole("button", { name: "Show all 49 selected references" }));
  scale = 2; width = 328;
  act(() => { for (const observer of [...observers]) observer.callback(); });
  for (let remaining = 49; remaining > 6; remaining--) fireEvent.click(removeButtons().at(-1)!);
  expect(list.hasAttribute("data-expanded")).toBe(true);
  fireEvent.click(within(list).getByRole("button", { name: "Show fewer selected references" }));
  expect(removeButtons()).toHaveLength(1);
  expect(within(list).getByRole("button", { name: "Show all 6 selected references" }).textContent).toBe("+5");
  scale = 1; width = 1000;
  act(() => { for (const observer of [...observers]) observer.callback(); });
  expect(within(list).queryByRole("button", { name: "Show all 6 selected references" })).toBeNull();
  removeButtons()[5]!.focus();
  width = 344;
  act(() => { for (const observer of [...observers]) observer.callback(); });
  expect(document.activeElement).toBe(within(list).getByRole("button", { name: "Show all 6 selected references" }));
  view.unmount();
  expect(observers.every((observer) => observer.disconnect.mock.calls.length > 0)).toBe(true);
});

test("the output is one line in text colours: keys in monospace, failures and pending in words", () => {
  const { rerender } = render(shelf({ keys: true, shown: { markup: [], text: "[@a; @b]", html: "", warnings: [] } }));
  const output = screen.getByRole("region", { name: "Preview" });
  expect(output.hasAttribute("data-keys")).toBe(true);
  expect(within(output).getByText("[@a; @b]").getAttribute("title")).toBe("[@a; @b]");
  rerender(shelf({ keys: false, style: "apa", shown: null }));
  expect(screen.getByRole("region", { name: "Preview" }).getAttribute("data-state")).toBe("pending");
  expect(within(screen.getByRole("region", { name: "Preview" })).getByRole("status").textContent).toBe("Formatting…");
  rerender(shelf({ keys: false, style: "apa", shown: "failed" }));
  expect(screen.getByRole("alert").textContent).toMatch(/could not be formatted/);
  rerender(shelf({ keys: false, style: "ieee", shown: { markup: [], text: "[1]", html: "", warnings: ["Missing year"] } }));
  expect(screen.getByText("Numbers may change")).toBeTruthy();
  expect(screen.getByText("1 warning")).toBeTruthy();
  expect(screen.getByText("Missing year")).toBeTruthy();
});

test("proportional overflow digits cannot cycle the fitted count, and growth restores chips", () => {
  let width = 350;
  const observers: (() => void)[] = [];
  vi.stubGlobal("ResizeObserver", class {
    constructor(callback: () => void) { observers.push(callback); }
    observe = vi.fn();
    disconnect = vi.fn();
  });
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(function (this: HTMLElement) {
    return this.matches(".shelf-chips") ? width : 0;
  });
  const computed = window.getComputedStyle;
  vi.spyOn(window, "getComputedStyle").mockImplementation((element) => {
    if (!element.matches(".chip, .shelf-chips")) return computed(element);
    const style = document.createElement("div").style;
    style.minWidth = "100px";
    style.columnGap = "4px";
    style.paddingInlineStart = style.paddingInlineEnd = "0px";
    return style;
  });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    // With three chips, +20 is wider and only two fit. With two, +21 is
    // narrower and three fit: measuring the current label never converges.
    return new DOMRect(0, 0, this.textContent === "+20" ? 40 : this.textContent === "+21" ? 30 : 35, 24);
  });
  const records = Array.from({ length: 23 }, (_, i) => record(`k${i}`, `Author${i}`));
  render(shelf({ selected: chosen(records) }));
  const list = screen.getByRole("list", { name: "Selected references" });
  const visible = () => within(list).getAllByRole("button", { name: /^Remove/ });
  const more = () => within(list).getByRole("button", { name: "Show all 23 selected references" });
  expect(visible()).toHaveLength(2);
  expect(more().textContent).toBe("+21");
  more().focus();
  act(() => { for (const observer of [...observers]) observer(); });
  expect(document.activeElement).toBe(more());
  expect(visible()).toHaveLength(2);
  width = 1000;
  act(() => { for (const observer of [...observers]) observer(); });
  expect(visible()).toHaveLength(6);
  width = 350;
  act(() => { for (const observer of [...observers]) observer(); });
  expect(visible()).toHaveLength(2);
  fireEvent.click(more());
  expect(visible().map((button) => button.getAttribute("aria-label"))).toEqual(records.map((r) => `Remove ${r.key}`));
});

test("nothing chosen and nothing to show draws no shelf", () => {
  const { container } = render(shelf({}));
  expect(container.querySelector(".shelf")).toBeNull();
});

test("the beak quote is decorative, unfocusable and confined to the output line", () => {
  const output = { markup: [], text: "Synthetic citation", html: "", warnings: [] };
  const { container, rerender } = render(shelf({ selected: chosen([record("a")]), shown: output, focusable: true }));
  const preview = screen.getByRole("region", { name: "Preview" });
  const mark = preview.querySelector("svg.shelf-mark");
  expect(mark?.getAttribute("viewBox")).toBe("23 25 58 51");
  expect(mark?.getAttribute("aria-hidden")).toBe("true");
  expect(mark?.getAttribute("focusable")).toBe("false");
  expect(mark?.querySelectorAll("circle")).toHaveLength(2);
  expect(mark?.textContent).toBe("");
  expect(preview.textContent).toBe(output.text);
  preview.focus();
  expect(document.activeElement).toBe(preview);
  rerender(shelf({ selected: chosen([record("a")]), shown: null }));
  expect(screen.getByRole("status", { name: "" }).textContent).toBe("Formatting…");
  expect(container.querySelectorAll("svg.shelf-mark")).toHaveLength(1);
  rerender(shelf({ selected: chosen([record("a")]) }));
  expect(container.querySelector(".shelf-mark")).toBeNull();
});

test("RTL mirrors the beak through its HTML shelf direction", () => {
  const sheet = document.createElement("style");
  sheet.textContent = readFileSync(new FileURL("./CitationShelf.css", import.meta.url), "utf8");
  // The build lowers :dir(rtl) to :lang(...), which misses explicit RTL
  // in an English UI. Keep the direction attribute in the built selector.
  expect(sheet.textContent).toContain('[dir="rtl"] .shelf-mark');
  document.head.append(sheet);
  try {
    const { container } = render(<div dir="rtl">{shelf({ shown: { markup: [], text: "Synthetic citation", html: "", warnings: [] } })}</div>);
    expect(getComputedStyle(container.querySelector("svg.shelf-mark")!).transform).toBe("scaleX(-1)");
  } finally { sheet.remove(); }
});
