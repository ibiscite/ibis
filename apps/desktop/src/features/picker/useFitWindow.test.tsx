import { act, cleanup, render } from "@testing-library/react";
import { StrictMode, useRef } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { useFitWindow, type FitPicker } from "./useFitWindow";

const originalFonts = Object.getOwnPropertyDescriptor(document, "fonts");
afterEach(() => { if (originalFonts) Object.defineProperty(document, "fonts", originalFonts); else Reflect.deleteProperty(document, "fonts"); document.documentElement.removeAttribute("data-text-size"); cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); document.querySelectorAll(".menu-popover, [role=dialog]").forEach((node) => node.remove()); });

function Picker({ compact, fit, rows, spare = 0, onShown, scroll = false }: { scroll?: boolean; compact: boolean; fit: FitPicker; rows: number; spare?: number; onShown?: (handler: () => void) => Promise<() => void> }) {
  const main = useRef<HTMLElement>(null);
  useFitWindow(main, compact, fit, onShown);
  return <main ref={main}><div className="picker-content" style={{ overflowY: scroll ? "auto" : undefined }}><div className="results-area">{Array.from({ length: rows }, (_, i) => <p key={i}>row</p>)}</div></div><footer style={{ marginBlockStart: `${spare}px` }} /></main>;
}

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
