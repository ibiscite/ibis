import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createRef, useState } from "react";
import type { SearchResult } from "../../api";
import { chipLabels, CitationShelf } from "./CitationShelf";

const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");

afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

function mockResizeObservers() {
  const observers: { callback: () => void; active: () => boolean }[] = [];
  vi.stubGlobal("ResizeObserver", class implements ResizeObserver {
    private nodes = new Set<Element>();
    observe(node: Element) { this.nodes.add(node); }
    unobserve(node: Element) { this.nodes.delete(node); }
    disconnect() { this.nodes.clear(); }
    constructor(callback: ResizeObserverCallback) {
      observers.push({ active: () => this.nodes.size > 0, callback: () => {
        if (!this.nodes.size) return;
        const entries = [...this.nodes].map((target) => {
          const contentRect = target.getBoundingClientRect();
          const size = [{ blockSize: contentRect.height, inlineSize: contentRect.width }];
          return { target, contentRect, borderBoxSize: size, contentBoxSize: size, devicePixelContentBoxSize: size };
        });
        callback(entries, this);
      } });
    }
  });
  return observers;
}

function record(key: string, first_author = "García", year: number | null = 2021): SearchResult {
  return {
    revision: "1", id: { source: "refs.bib", native_key: key }, key, title: `Title ${key}`, authors: first_author,
    year, matched: ["title"], partial: false, first_author, author_count: 1, container: null, item_type: "book",
  };
}

function chosen(records: SearchResult[]) {
  return new Map(records.map((r) => [r.key, r]));
}

const shelf = (props: Partial<Parameters<typeof CitationShelf>[0]>) => (
  <CitationShelf announce={false} selected={new Map()} removeLabel={(r) => `Remove ${r.key}`} onRemove={() => undefined}
    style={null} keys shown={undefined} focusable={false}
    outputWarnings={props.shown && typeof props.shown === "object" ? props.shown.warnings : []} {...props} />
);

test("chips read as author and year, and add the key only where two would read the same", () => {
  expect(chipLabels([record("a"), record("b", "Okafor", 2019), record("c", "", null)])).toEqual(["García 2021", "Okafor 2019", "c"]);
  expect(chipLabels([record("garcia2021a"), record("garcia2021b")])).toEqual(["García 2021 · garcia2021a", "García 2021 · garcia2021b"]);
});

test("the failed citation shows its actionable refusal beside the citation before Details opens", () => {
  const reason = "Reference 1: Narrative cannot be expressed here. Choose Normal or another output.";
  render(shelf({ shown: "failed", problem: reason }));
  expect(within(screen.getByRole("region", { name: "Preview" })).getByText(reason)).toBeTruthy();
  expect(screen.queryByRole("dialog")).toBeNull();
});

test("a reference opens an anchored menu on its own segment and Escape returns there", async () => {
  render(shelf({ selected: chosen([record("a"), record("b")]), onChangeReference: vi.fn() }));
  const edit = screen.getByRole("button", { name: "Edit García 2021 · b" });
  act(() => edit.focus());
  fireEvent.click(edit);
  const details = screen.getByRole("dialog", { name: "Citation details for Title b" });
  expect(details.closest(".details-menu")).not.toBeNull();
  expect(document.querySelector(".overlay-backdrop")).toBeNull();
  expect(edit.getAttribute("aria-expanded")).toBe("true");
  expect(document.activeElement).toBe(within(details).getByRole("textbox", { name: "Page" }));
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  await act(async () => { await new Promise(requestAnimationFrame); });
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(document.activeElement).toBe(edit);
});

test.each([[1, true], [2, true], [1, false], [2, false]] as const)("folding at %s× text keeps warnings, reference entries and closest matches reachable (Preview=%s)", (scale, preview) => {
  vi.stubGlobal("innerWidth", 480);
  vi.stubGlobal("innerHeight", 360);
  const computed = window.getComputedStyle;
  vi.spyOn(window, "getComputedStyle").mockImplementation((element) => {
    if (element !== document.documentElement) return computed(element);
    const style = document.createElement("div").style;
    style.fontSize = `${16 * scale}px`;
    return style;
  });
  const reference = { ...record("near"), title: "Synthetic closest match", partial: true };
  const warnings = ["Reference 1: Page is not included in this reference entry."];
  render(shelf({ selected: chosen([reference]), style: "ieee", keys: false, outputWarnings: warnings,
    shown: preview ? { markup: [], text: "Synthetic entry", html: "", warnings } : undefined }));
  const control = screen.getByRole("button", { name: /2 warnings.*Reference entry.*Closest matches/ });
  act(() => control.focus());
  fireEvent.keyDown(control, { key: " ", code: "Space" });
  fireEvent.keyUp(control, { key: " ", code: "Space" });
  const details = screen.getByRole("dialog", { name: "Citation warnings" });
  expect(within(details).getByText("Reference entry")).toBeTruthy();
  expect(within(details).getByText("Closest matches")).toBeTruthy();
  expect(within(details).getByText("Reference 1: Page is not included in this reference entry.")).toBeTruthy();
  expect(within(details).getByText(/IEEE numbers depend on your whole document/)).toBeTruthy();
  expect(within(details).getByText(/only some search words matched/)).toBeTruthy();
});

test("expanding chosen references opens an overlay instead of growing the shelf", async () => {
  render(shelf({ selected: chosen(Array.from({ length: 9 }, (_, i) => record(`k${i}`))) }));
  const band = document.querySelector(".shelf")!;
  fireEvent.click(screen.getByRole("button", { name: "Show all 9 selected references" }));
  const overlay = await screen.findByRole("dialog", { name: "Selected references" });
  expect(band.contains(overlay)).toBe(false);
  expect(band.querySelector("[data-expanded]")).toBeNull();
  expect(within(overlay).getAllByRole("button", { name: /^Remove/ })).toHaveLength(9);
});

test.each([1, 2])("at the minimum frame and %s× text, every folded reference stays removable in the overlay", (scale) => {
  vi.stubGlobal("innerWidth", 480);
  vi.stubGlobal("innerHeight", 360);
  const computed = window.getComputedStyle;
  vi.spyOn(window, "getComputedStyle").mockImplementation((element) => {
    if (element !== document.documentElement) return computed(element);
    const style = document.createElement("div").style;
    style.fontSize = `${16 * scale}px`;
    return style;
  });
  const records = Array.from({ length: 9 }, (_, i) => record(`k${i}`));
  const remove = vi.fn();
  render(shelf({ selected: chosen(records), onRemove: remove, shown: { markup: [], text: "Synthetic citation", html: "", warnings: [] } }));
  const band = document.querySelector(".shelf")!;
  expect(band.hasAttribute("data-folded")).toBe(true);
  expect(screen.queryByRole("list", { name: "Selected references" })).toBeNull();
  expect(screen.getByRole("region", { name: "Preview" }).textContent).toContain("Synthetic citation");
  fireEvent.click(screen.getByRole("button", { name: "Show all 9 selected references" }));
  const dialog = screen.getByRole("dialog", { name: "Selected references" });
  expect(band.contains(dialog)).toBe(false);
  for (const r of records) fireEvent.click(within(dialog).getByRole("button", { name: `Remove ${r.key}` }));
  expect(remove.mock.calls.map(([r]) => r)).toEqual(records);
});

test("a long reference title stays available in the details scroller beside its editor", () => {
  const reference = { ...record("a"), title: "A synthetic title with many words. ".repeat(30).trim() };
  render(shelf({ highlightedReference: reference, onChangeReference: vi.fn() }));
  fireEvent.click(screen.getByRole("button", { name: "Edit citation" }));
  const dialog = screen.getByRole("dialog", { name: `Citation details for ${reference.title}` });
  expect(within(dialog).getByRole("heading").textContent).toBe(reference.title);
  const scroll = dialog.querySelector<HTMLElement>(".details-menu-scroll")!;
  expect(within(scroll).getByText(reference.title)).toBeTruthy();
  expect(within(scroll).getByRole("textbox", { name: "Page" })).toBeTruthy();
});

test("closest-match chips and announcements keep provenance with the preview hidden", () => {
  const partial = { ...record("partial"), title: "Synthetic near match", partial: true };
  const { rerender } = render(shelf({ announce: true, selected: chosen([record("exact"), partial]) }));
  const chip = screen.getByRole("button", { name: "Remove partial" }).closest("li")!;
  expect(within(chip).getByText("Closest match").className).not.toContain("visually-hidden");
  expect(within(chip).getByText(/only some search words matched/).className).toBe("visually-hidden");
  expect(within(chip).getByText("Closest match").parentElement?.className).toBe("shelf-pill");
  expect(screen.getByRole("status", { name: "Citation selection" }).textContent).toContain("Synthetic near match");
  rerender(shelf({ selected: chosen([record("exact")]) }));
  expect(screen.queryByText("Closest match")).toBeNull();
});

test("folded closest matches remain marked and their warning uses the existing preview pill", () => {
  const records = Array.from({ length: 8 }, (_, i) => ({ ...record(`k${i}`), partial: i === 7 }));
  render(shelf({ selected: chosen(records), shown: { markup: [], text: "Synthetic citation", html: "", warnings: [] } }));
  const more = screen.getByRole("button", { name: "Show all 8 selected references" });
  expect(more.textContent).toBe("+2 · Closest matches");
  expect(more.getAttribute("title")).toContain("only some search words matched");
  const output = screen.getByRole("region", { name: "Preview" });
  expect(within(output).getByText("1 warning")).toBeTruthy();
  expect(within(output).getByText(/only some search words matched/)).toBeTruthy();
  fireEvent.click(more);
  const chip = screen.getByRole("button", { name: "Remove k7" }).closest("li")!;
  expect(within(chip).getByText("Closest match")).toBeTruthy();
});

test("marked chips fold without squeezing the warning or cycling when hidden, then return on growth", () => {
  let width = 350;
  const computed = window.getComputedStyle;
  vi.spyOn(window, "getComputedStyle").mockImplementation((element) => {
    if (!element.matches(".chip, .shelf-chips")) return computed(element);
    const style = document.createElement("div").style;
    style.minWidth = "80px";
    style.maxWidth = "192px";
    style.columnGap = "4px";
    style.paddingInlineStart = style.paddingInlineEnd = "12px";
    return style;
  });
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(function (this: HTMLElement) {
    return this.classList.contains("shelf-chips") ? width : 0;
  });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    return { width: this.matches(".chip-fit-measure > button") ? 120 : this.hasAttribute("data-partial") && !this.hidden ? 192 : 0 } as DOMRect;
  });
  render(shelf({ selected: chosen([record("a"), record("b"), { ...record("near"), partial: true }]) }));
  const list = () => screen.getByRole("list", { name: "Selected references" });
  expect(within(list()).getAllByRole("button", { name: /^Remove/ })).toHaveLength(1);
  expect(within(list()).getByRole("button", { name: "Show all 3 selected references" }).textContent).toContain("Closest matches");
  width = 650;
  fireEvent(window, new Event("resize"));
  expect(within(list()).getAllByRole("button", { name: /^Remove/ })).toHaveLength(3);
  expect(within(list()).getByText("Closest match")).toBeTruthy();
});

test("six chips show, the rest fold into +N, and +N shows every chip so each can be removed", () => {
  const records = Array.from({ length: 9 }, (_, i) => record(`k${i}`, `Author${i}`));
  const onRemove = vi.fn();
  render(shelf({ selected: chosen(records), onRemove }));
  const list = () => screen.getByRole("list", { name: "Selected references" });
  expect(within(list()).getAllByRole("button", { name: /^Remove/ })).toHaveLength(6);
  const more = within(list()).getByRole("button", { name: "Show all 9 selected references" });
  expect(more.textContent).toBe("+3");
  expect(more.getAttribute("aria-expanded")).toBe("false");
  fireEvent.click(more);
  expect(within(list()).getAllByRole("button", { name: /^Remove/ })).toHaveLength(9);
  fireEvent.click(within(list()).getByRole("button", { name: "Remove k8" }));
  expect(onRemove).toHaveBeenCalledWith(records[8]);
  // Each chip names its whole key to the pointer.
  expect(within(list()).getByText("Author8 2021").closest("li")?.getAttribute("title")).toBe("k8");
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
  press(screen.getByRole("button", { name: "Close" }));
  expect(screen.getAllByRole("button", { name: /^Remove/ })).toHaveLength(6);
});

test("50 choices fit narrow and 200% text rows, keeping hidden source identities removable", () => {
  let scale = 1;
  let width = 344;
  const observers = mockResizeObservers();
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
    return shelf({ selected, removeLabel: (r) => { const id = r.id as { source: string; native_key: string }; return `Remove ${id.source}/${id.native_key}`; }, onRemove: (r) => {
      onRemove(r); setSelected((previous) => new Map([...previous].filter(([, value]) => value !== r)));
    } });
  }
  const view = render(<Picker />);
  const list = () => screen.getByRole("list", { name: "Selected references" });
  const removeButtons = () => within(list()).getAllByRole("button", { name: /^Remove/ });
  expect(removeButtons()).toHaveLength(3);
  expect(within(list()).getByRole("button", { name: "Show all 50 selected references" }).textContent).toBe("+47");
  scale = 2; width = 328;
  act(() => { for (const observer of [...observers]) observer.callback(); });
  expect(removeButtons()).toHaveLength(1);
  const more = within(list()).getByRole("button", { name: "Show all 50 selected references" });
  expect(more.textContent).toBe("+49");
  fireEvent.click(more);
  expect(removeButtons().map((button) => button.getAttribute("aria-label"))).toEqual(records.map((r) => `Remove ${r.id.source}/${r.id.native_key}`));
  fireEvent.click(within(list()).getByRole("button", { name: "Remove second.bib/entry49" }));
  expect(onRemove).toHaveBeenCalledWith(records[49]);
  expect(removeButtons()).toHaveLength(49);
  expect(within(list()).getByRole("button", { name: "Remove first.bib/entry24" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Close" }));
  expect(within(list()).getByRole("button", { name: "Show all 49 selected references" }).textContent).toBe("+48");
  scale = 1; width = 1000;
  act(() => { for (const observer of [...observers]) observer.callback(); });
  expect(removeButtons()).toHaveLength(6);
  expect(within(list()).getByRole("button", { name: "Show all 49 selected references" }).textContent).toBe("+43");
  act(() => removeButtons()[5]!.focus());
  width = 344;
  act(() => { for (const observer of [...observers]) observer.callback(); });
  expect(removeButtons()).toHaveLength(3);
  expect(document.activeElement).toBe(within(list()).getByRole("button", { name: "Show all 49 selected references" }));
  width = 1000;
  act(() => { for (const observer of [...observers]) observer.callback(); });
  fireEvent.click(within(list()).getByRole("button", { name: "Show all 49 selected references" }));
  scale = 2; width = 328;
  act(() => { for (const observer of [...observers]) observer.callback(); });
  // Retained chips keep their DOM identity. Avoid recomputing accessible names
  // and visibility for every remaining button after each of 43 removals.
  const tail = removeButtons().slice(6).reverse();
  for (const button of tail) fireEvent.click(button);
  expect(list().className === "shelf-details-list").toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "Close" }));
  expect(removeButtons()).toHaveLength(1);
  expect(within(list()).getByRole("button", { name: "Show all 6 selected references" }).textContent).toBe("+5");
  scale = 1; width = 1000;
  act(() => { for (const observer of [...observers]) observer.callback(); });
  expect(within(list()).queryByRole("button", { name: "Show all 6 selected references" })).toBeNull();
  act(() => removeButtons()[5]!.focus());
  width = 344;
  act(() => { for (const observer of [...observers]) observer.callback(); });
  expect(document.activeElement).toBe(within(list()).getByRole("button", { name: "Show all 6 selected references" }));
  view.unmount();
  expect(observers.every((observer) => !observer.active())).toBe(true);
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
  expect(screen.getByText(/This citation could not be formatted/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Details" }));
  expect(within(screen.getByRole("dialog")).getByText(/could not be formatted/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Close" }));
  rerender(shelf({ keys: false, style: "ieee", shown: { markup: [], text: "A. Garcia, “Synthetic,” 2021.", html: "", warnings: ["Missing year"] } }));
  expect(screen.getByText("Reference entry")).toBeTruthy();
  expect(screen.getByText("IEEE numbers depend on your whole document, so Ibis copies the reference entry instead.")).toBeTruthy();
  expect(screen.getByText("1 warning")).toBeTruthy();
  expect(screen.getByText("Missing year")).toBeTruthy();
});

test("proportional overflow digits cannot cycle the fitted count, and growth restores chips", () => {
  let width = 350;
  const observers = mockResizeObservers();
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
  const list = () => screen.getByRole("list", { name: "Selected references" });
  const visible = () => within(list()).getAllByRole("button", { name: /^Remove/ });
  const more = () => within(list()).getByRole("button", { name: "Show all 23 selected references" });
  expect(visible()).toHaveLength(2);
  expect(more().textContent).toBe("+21");
  act(() => more().focus());
  act(() => { for (const observer of [...observers]) observer.callback(); });
  expect(document.activeElement).toBe(more());
  expect(visible()).toHaveLength(2);
  width = 1000;
  act(() => { for (const observer of [...observers]) observer.callback(); });
  expect(visible()).toHaveLength(6);
  width = 350;
  act(() => { for (const observer of [...observers]) observer.callback(); });
  expect(visible()).toHaveLength(2);
  fireEvent.click(more());
  expect(visible().map((button) => button.getAttribute("aria-label"))).toEqual(records.map((r) => `Remove ${r.key}`));
});

test("nothing chosen and nothing to show draws no shelf", () => {
  const { container } = render(shelf({}));
  expect(container.querySelector(".shelf")).toBeNull();
});

test.each(["removed", "retained"])("a programmatically removed chip hands off only its own focus (%s)", (focus) => {
  const fallbackFocusRef = createRef<HTMLInputElement>();
  const records = [record("a"), record("b")];
  const picker = (remaining: SearchResult[]) => <>
    <input aria-label="Search" ref={fallbackFocusRef} />
    {shelf({ selected: chosen(remaining), announce: true, fallbackFocusRef })}
  </>;
  const view = render(picker(records));
  const target = screen.getByRole("button", { name: focus === "removed" ? "Remove a" : "Remove b" });
  act(() => target.focus());
  // An ordinary rerender must keep focus on the current chip.
  view.rerender(picker([...records]));
  expect(document.activeElement).toBe(target);
  view.rerender(picker(records.slice(1)));
  expect(document.activeElement).toBe(focus === "removed" ? fallbackFocusRef.current : target);
  expect(screen.getByRole("button", { name: "Remove b" })).toBeTruthy();
  expect(screen.getByRole("status", { name: "Citation selection" }).textContent).toBe("1 reference selected for citation");
});

test("the beak quote is decorative, unfocusable and confined to the output line", () => {
  const output = { markup: [], text: "Synthetic citation", html: "", warnings: [] };
  const { container, rerender } = render(shelf({ selected: chosen([record("a")]), shown: output, focusable: true }));
  const preview = screen.getByRole("region", { name: "Preview" });
  const mark = preview.querySelector("svg.shelf-mark");
  const master = new DOMParser().parseFromString(readFileSync(new FileURL("../../../../../brand/master.svg", import.meta.url), "utf8"), "image/svg+xml");
  expect(mark?.getAttribute("viewBox")).toBe(master.querySelector('[id="beak"]')?.getAttribute("viewBox"));
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


test("a captured locator has a separate removal action naming its owning reference", () => {
  const reference = { ...record("a"), locator: { label: "page" as const, value: "12–15" } };
  const onRemove = vi.fn(), onRemoveLocator = vi.fn();
  render(shelf({ selected: chosen([reference]), onRemove, onRemoveLocator }));
  expect(screen.getByText("page 12–15").tagName).toBe("BDI");
  fireEvent.click(screen.getByRole("button", { name: "Remove page 12–15 from a" }));
  expect(onRemoveLocator).toHaveBeenCalledExactlyOnceWith(reference);
  expect(onRemove).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "Remove a" })).toBeTruthy();
});
