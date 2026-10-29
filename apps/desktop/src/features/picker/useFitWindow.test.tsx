import { act, cleanup, render } from "@testing-library/react";
import { StrictMode, useRef } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { useFitWindow, type FitPicker } from "./useFitWindow";

afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); document.querySelectorAll(".menu-popover, [role=dialog]").forEach((node) => node.remove()); });

function Picker({ compact, fit, rows, spare = 0, onShown }: { compact: boolean; fit: FitPicker; rows: number; spare?: number; onShown?: (handler: () => void) => Promise<() => void> }) {
  const main = useRef<HTMLElement>(null);
  useFitWindow(main, compact, fit, onShown);
  return <main ref={main}><div className="results-area">{Array.from({ length: rows }, (_, i) => <p key={i}>row</p>)}</div><footer style={{ marginBlockStart: `${spare}px` }} /></main>;
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
