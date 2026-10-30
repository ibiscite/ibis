import { act, cleanup, fireEvent, render } from "@testing-library/react";
import { StrictMode, useRef } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { useFitWindow, type FitPicker } from "./useFitWindow";

const originalFonts = Object.getOwnPropertyDescriptor(document, "fonts");
afterEach(() => { if (originalFonts) Object.defineProperty(document, "fonts", originalFonts); else Reflect.deleteProperty(document, "fonts"); document.documentElement.removeAttribute("data-text-size"); cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); document.querySelectorAll(".menu-popover, [role=dialog]").forEach((node) => node.remove()); });

function Picker({ compact, fit, rows, spare = 0, onShown, scroll = false, notice, generation = 0 }: { notice?: string; scroll?: boolean; compact: boolean; fit: FitPicker; rows: number; spare?: number; onShown?: (handler: () => void) => Promise<() => void>; generation?: number }) {
  const main = useRef<HTMLElement>(null);
  useFitWindow(main, compact, fit, onShown);
  return <main ref={main}><header className="search-bar"><input id="search" aria-label="Synthetic search" /></header><div className="picker-content" style={{ containerType: "size", overflowY: scroll ? "auto" : undefined }}>{notice && <div className="notices"><p role="alert">{notice}</p></div>}<div className="results-area">{Array.from({ length: rows }, (_, i) => <p role="option" id={`result-${i}`} key={`${generation}/${i}`}>row</p>)}</div></div><footer style={{ marginBlockStart: `${spare}px` }} /></main>;
}

test.each([false, true])("notice appear and clear preserve only the current browsing anchor (replacement=%s)", (replacement) => {
  // Model the reported 720x480 layout: row 8 is at y=189 after scrolling 380px,
  // and would jump to y=611 when a 42px notice disables results scrolling.
  // The browser probe establishes the real CSS geometry and pointer selection.
  const scrollTop = Object.getOwnPropertyDescriptor(Element.prototype, "scrollTop")!;
  const hasNotice = (node: HTMLElement) => !!node.closest("main")?.querySelector(".notices");
  const noticeHeight = (node: HTMLElement) => hasNotice(node) ? node.closest("main")!.querySelector(".notices")!.textContent?.startsWith("Expanded") ? 84 : 42 : 0;
  const computed = window.getComputedStyle;
  vi.spyOn(window, "getComputedStyle").mockImplementation((node) => {
    if (!node.matches(".results-area, .picker-content")) return computed(node);
    const style = document.createElement("div").style;
    style.overflowY = node.matches(".results-area") && hasNotice(node as HTMLElement) ? "visible" : "auto";
    return style;
  });
  vi.spyOn(HTMLElement.prototype, "scrollTop", "get").mockImplementation(function (this: HTMLElement) {
    // Browsers clamp an offset when its element stops being a scrollport.
    if (this.matches(".results-area") && hasNotice(this) || this.matches(".picker-content") && !hasNotice(this)) scrollTop.set!.call(this, 0);
    return scrollTop.get!.call(this);
  });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.matches(".notices")) return new DOMRect(0, 53, 720, noticeHeight(this));
    if (this.matches("[role=option]")) {
      const main = this.closest("main")!;
      const results = main.querySelector<HTMLElement>(".results-area")!;
      const middle = main.querySelector<HTMLElement>(".picker-content")!;
      return new DOMRect(8, 89 + Number(this.id.slice(7)) * 60 + noticeHeight(this) - results.scrollTop - middle.scrollTop, 704, 58);
    }
    return new DOMRect();
  });
  const fit = vi.fn<FitPicker>(() => Promise.resolve());
  const view = render(<Picker compact={false} fit={fit} rows={14} scroll />);
  const results = view.container.querySelector<HTMLElement>(".results-area")!;
  const middle = view.container.querySelector<HTMLElement>(".picker-content")!;
  const row = view.container.querySelector<HTMLElement>("#result-8")!;
  results.scrollTop = 380; fireEvent.scroll(results);
  expect(row.getBoundingClientRect().top).toBe(189);
  view.rerender(<Picker compact={false} fit={fit} rows={14} scroll notice="Synthetic update progress" generation={replacement ? 1 : 0} />);
  if (replacement) {
    expect(row.isConnected).toBe(false);
    expect(view.container.querySelector("#result-0")!.getBoundingClientRect().top).toBe(131);
    expect([results.scrollTop, middle.scrollTop]).toEqual([0, 0]);
    return;
  }
  expect(row.getBoundingClientRect().top).toBe(189);
  expect([results.scrollTop, middle.scrollTop]).toEqual([0, 422]);
  view.rerender(<Picker compact={false} fit={fit} rows={14} scroll notice="Expanded synthetic update progress" />);
  expect(row.getBoundingClientRect().top).toBe(189);
  expect([results.scrollTop, middle.scrollTop]).toEqual([0, 464]);
  // Browse elsewhere while the notice is present; clearing must retain that
  // newer place rather than restoring the pre-notice offset.
  middle.scrollTop += 60; fireEvent.scroll(middle);
  expect(row.getBoundingClientRect().top).toBe(129);
  view.rerender(<Picker compact={false} fit={fit} rows={14} scroll />);
  expect(row.getBoundingClientRect().top).toBe(129);
  expect([results.scrollTop, middle.scrollTop]).toEqual([440, 0]);
});

test.each([32, 64].flatMap((noticeHeight) => ["startup", "appearing"].map((when) => ({ noticeHeight, when }))))("a $when compact notice contributes its full $noticeHeight px beyond a size-contained middle", ({ noticeHeight, when }) => {
  // Model the reported layout boundary: size containment hides a shrinking
  // notice from the middle's scrollHeight, despite its own nonzero content.
  // jsdom has no flex layout; the browser probe checks the real CSS geometry.
  const middleHeight = (node: HTMLElement) => {
    const middle = node.querySelector<HTMLElement>(".picker-content")!;
    return getComputedStyle(middle).containerType === "size" ? 0 : middle.querySelector(".notices") ? noticeHeight : 0;
  };
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this.tagName === "MAIN" ? 109 + middleHeight(this) : 0;
  });
  vi.spyOn(HTMLElement.prototype, "scrollHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this.tagName === "MAIN" ? 109 + middleHeight(this) : this.classList.contains("notices") ? noticeHeight : 0;
  });
  const fit = vi.fn<FitPicker>(() => Promise.resolve());
  const view = render(<Picker compact fit={fit} rows={0} scroll notice={when === "startup" ? "Synthetic settings notice" : undefined} />);
  if (when === "appearing") {
    expect(fit).toHaveBeenLastCalledWith(109, expect.any(Boolean));
    view.rerender(<Picker compact fit={fit} rows={0} scroll notice="Synthetic settings notice" />);
  }
  expect(fit).toHaveBeenLastCalledWith(109 + noticeHeight, expect.any(Boolean));
  expect(view.container.querySelector<HTMLElement>(".picker-content")!.style.containerType).toBe("size");
});

test("remeasuring for More preserves scrolled footer, nested offsets, focus and selection", async () => {
  // A browser clamps scroll offsets when the measured auto-height frame grows
  // to its natural height. Restoring cssText alone cannot restore those offsets.
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
    if (this.tagName === "MAIN" && this.style.height === "auto") {
      this.scrollTop = this.scrollLeft = 0;
      const middle = this.querySelector<HTMLElement>(".picker-content")!;
      middle.scrollTop = middle.scrollLeft = 0;
    }
    return this.tagName === "MAIN" ? 900 : 0;
  });
  const fit = vi.fn<FitPicker>(() => Promise.resolve());
  const view = render(<Picker compact fit={fit} rows={0} />);
  const main = view.container.querySelector("main")!;
  const middle = view.container.querySelector<HTMLElement>(".picker-content")!;
  const input = view.container.querySelector("input")!;
  input.value = "invented";
  input.focus(); input.setSelectionRange(2, 5);
  main.scrollTop = 580; main.scrollLeft = 4;
  middle.scrollTop = 60; middle.scrollLeft = 8;
  const menu = document.createElement("div");
  menu.className = "menu-popover"; menu.setAttribute("role", "dialog");
  await act(async () => { document.body.append(menu); });
  expect([main.scrollTop, main.scrollLeft, middle.scrollTop, middle.scrollLeft]).toEqual([580, 4, 60, 8]);
  expect(document.activeElement).toBe(input);
  expect([input.selectionStart, input.selectionEnd]).toEqual([2, 5]);
  expect(fit).toHaveBeenCalledTimes(1);
  await act(async () => { menu.remove(); });
  expect(main.scrollTop).toBe(580);
});

test("compact content asks for its own height, results for the results height, once each", () => {
  const fit = vi.fn<FitPicker>(() => Promise.resolve());
  const height = vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(132.4);
  const { rerender } = render(<Picker compact fit={fit} rows={0} />);
  expect(fit).toHaveBeenLastCalledWith(133, expect.any(Boolean));
  rerender(<Picker compact fit={fit} rows={0} />);
  expect(fit).toHaveBeenCalledTimes(1);
  rerender(<Picker compact={false} fit={fit} rows={3} />);
  expect(fit).toHaveBeenLastCalledWith(null, expect.any(Boolean));
  height.mockRestore();
});

test("the space that pushes the footer down is not counted as content", () => {
  const fit = vi.fn<FitPicker>(() => Promise.resolve());
  const height = vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(480);
  render(<Picker compact fit={fit} rows={0} spare={339} />);
  expect(fit).toHaveBeenLastCalledWith(141, expect.any(Boolean));
  height.mockRestore();
});

test.each(["menu-popover", "menu-popover toolbar-menu-popover"])("opening and closing %s preserves the compact fit request", async (className) => {
  const fit = vi.fn<FitPicker>(() => Promise.resolve());
  render(<Picker compact fit={fit} rows={0} />);
  const menu = document.createElement("div");
  menu.className = className;
  menu.setAttribute("role", "dialog"); // React Aria gives menu popovers this role too.
  await act(async () => { document.body.append(menu); await Promise.resolve(); });
  expect(fit).toHaveBeenCalledTimes(1);
  await act(async () => { menu.remove(); });
  expect(fit).toHaveBeenCalledTimes(1);
});

test("reduced motion resizes without animation", () => {
  const fit = vi.fn<FitPicker>(() => Promise.resolve());
  vi.stubGlobal("matchMedia", (query: string) => ({ matches: query.includes("reduce") }));
  render(<Picker compact={false} fit={fit} rows={1} />);
  expect(fit).toHaveBeenLastCalledWith(null, false);
  vi.unstubAllGlobals();
});


test("explicit dialogs still request room and closing returns to content sizing", async () => {
  const fit = vi.fn<FitPicker>(() => Promise.resolve());
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(140);
  render(<Picker compact fit={fit} rows={0} />);
  const dialog = document.createElement("div");
  dialog.setAttribute("role", "dialog");
  await act(async () => { document.body.append(dialog); });
  expect(fit).toHaveBeenLastCalledWith(null, expect.any(Boolean));
  await act(async () => { dialog.remove(); });
  expect(fit).toHaveBeenLastCalledWith(140, expect.any(Boolean));
});

test("a current failed fit retries at the next measurement without polling", async () => {
  const fit = vi.fn<FitPicker>().mockRejectedValueOnce(new Error("synthetic fit refusal")).mockResolvedValue(undefined);
  const view = render(<StrictMode><Picker compact fit={fit} rows={0} /></StrictMode>);
  await act(async () => { await Promise.resolve(); });
  view.rerender(<StrictMode><Picker compact fit={fit} rows={0} /></StrictMode>);
  expect(fit).toHaveBeenCalledTimes(2);
});

test.each(["content", "reopen"])("a delayed failed fit cannot invalidate the newer %s request", async (next) => {
  let reject!: (error: unknown) => void;
  let shown!: () => void;
  const first = new Promise((_, fail) => { reject = fail; });
  const fit = vi.fn<FitPicker>().mockReturnValueOnce(first).mockResolvedValue(undefined);
  const onShown = async (handler: () => void) => { shown = handler; return () => {}; };
  const view = render(<Picker compact fit={fit} rows={0} onShown={onShown} />);
  if (next === "reopen") act(() => shown());
  else view.rerender(<Picker compact={false} fit={fit} rows={2} onShown={onShown} />);
  expect(fit).toHaveBeenCalledTimes(2);
  await act(async () => { reject(new Error("synthetic old fit refusal")); });
  view.rerender(<Picker compact={next === "reopen"} fit={fit} rows={2} onShown={onShown} />);
  expect(fit).toHaveBeenCalledTimes(2);
});


test.each(["normal", "larger"])("%s compact content grows past a clipped viewport instead of fitting the clipped box", (size) => {
  document.documentElement.dataset.textSize = size;
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(109);
  vi.spyOn(HTMLElement.prototype, "scrollHeight", "get").mockReturnValue(size === "larger" ? 151 : 117);
  const fit = vi.fn<FitPicker>(() => Promise.resolve());
  render(<Picker compact fit={fit} rows={0} />);
  expect(fit).toHaveBeenLastCalledWith(size === "larger" ? 151 : 117, expect.any(Boolean));
});

test("a constrained middle scrollport contributes its hidden content exactly once", () => {
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(320);
  vi.spyOn(HTMLElement.prototype, "scrollHeight", "get").mockImplementation(function (this: HTMLElement) { return this.classList.contains("picker-content") ? 400 : 320; });
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockImplementation(function (this: HTMLElement) { return this.classList.contains("picker-content") ? 200 : 320; });
  const fit = vi.fn<FitPicker>(() => Promise.resolve());
  render(<Picker compact fit={fit} rows={0} scroll />);
  expect(fit).toHaveBeenLastCalledWith(520, expect.any(Boolean));
});

test("font readiness and text-size changes remeasure the latest content without polling", async () => {
  let ready!: () => void;
  const fonts = Object.assign(new EventTarget(), { ready: new Promise<void>((resolve) => { ready = resolve; }) });
  Object.defineProperty(document, "fonts", { configurable: true, value: fonts });
  const height = vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(109);
  const fit = vi.fn<FitPicker>(() => Promise.resolve());
  render(<Picker compact fit={fit} rows={0} />);
  height.mockReturnValue(173);
  await act(async () => { ready(); });
  expect(fit).toHaveBeenLastCalledWith(173, expect.any(Boolean));
  height.mockReturnValue(191);
  await act(async () => { document.documentElement.dataset.textSize = "larger"; });
  expect(fit).toHaveBeenLastCalledWith(191, expect.any(Boolean));
});

test("a font completion after unmount cannot resize a reopened picker", async () => {
  let ready!: () => void;
  const fonts = Object.assign(new EventTarget(), { ready: new Promise<void>((resolve) => { ready = resolve; }) });
  Object.defineProperty(document, "fonts", { configurable: true, value: fonts });
  const first = vi.fn<FitPicker>(() => Promise.resolve());
  const old = render(<Picker compact fit={first} rows={0} />);
  old.unmount();
  const reopened = vi.fn<FitPicker>(() => Promise.resolve());
  render(<Picker compact={false} fit={reopened} rows={2} />);
  const previousCalls = first.mock.calls.length;
  await act(async () => { ready(); });
  expect(first).toHaveBeenCalledTimes(previousCalls);
  expect(reopened).toHaveBeenCalledExactlyOnceWith(null, expect.any(Boolean));
});


test("measuring content restores the visible frame and does not remember unused viewport space", () => {
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) { return this.style.height === "auto" ? 173 : 320; });
  const fit = vi.fn<FitPicker>(() => Promise.resolve());
  const view = render(<Picker compact fit={fit} rows={0} />);
  expect(fit).toHaveBeenLastCalledWith(173, expect.any(Boolean));
  expect(view.container.querySelector("main")!.style.cssText).toBe("");
});


test("extreme text uses one reachable page when fixed edges cannot leave a readable middle control", () => {
  let header = 397, footer = 187;
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(320);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this.tagName === "HEADER" ? header : this.tagName === "FOOTER" ? footer : this.tagName === "INPUT" ? 32 : 320;
  });
  const fit = vi.fn<FitPicker>(() => Promise.resolve());
  const view = render(<Picker compact={false} fit={fit} rows={2} />);
  const main = view.container.querySelector("main")!;
  expect(main.hasAttribute("data-scroll-page")).toBe(true);
  header = 57; footer = 40;
  view.rerender(<Picker compact={false} fit={fit} rows={2} />);
  expect(main.hasAttribute("data-scroll-page")).toBe(false);
  expect(fit).toHaveBeenCalledTimes(1);
});


test.each([88, 120])("growing from %spx leaves fallback without needing a content rerender", (smallHeight) => {
  let frame = smallHeight, notify = () => {};
  const observed = new Set<Element>();
  vi.stubGlobal("ResizeObserver", class {
    constructor(callback: ResizeObserverCallback) { notify = () => callback([], this as unknown as ResizeObserver); }
    observe(node: Element) { observed.add(node); }
    disconnect() { observed.clear(); }
  });
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockImplementation(() => frame);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this.tagName === "HEADER" ? 57 : this.tagName === "FOOTER" ? 40 : this.tagName === "INPUT" ? 32 : frame;
  });
  const fit = vi.fn<FitPicker>(() => Promise.resolve());
  const view = render(<Picker compact={false} fit={fit} rows={14} />);
  const main = view.container.querySelector("main")!;
  expect(main.hasAttribute("data-scroll-page")).toBe(true);
  frame = 320;
  act(() => { if (observed.has(main)) notify(); });
  expect(main.hasAttribute("data-scroll-page")).toBe(false);
  expect(fit).toHaveBeenCalledTimes(1);
  const retired = notify;
  view.unmount();
  const reopened = render(<Picker compact={false} fit={fit} rows={14} />);
  const results = reopened.container.querySelector<HTMLElement>(".results-area")!;
  results.scrollTop = 380; fireEvent.scroll(results);
  const calls = fit.mock.calls.length;
  act(() => retired()); // a queued callback after disconnect has no owner.
  expect(results.scrollTop).toBe(380);
  expect(fit).toHaveBeenCalledTimes(calls);
});
