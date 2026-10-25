import { act, render } from "@testing-library/react";
import { useRef } from "react";
import { expect, test, vi } from "vitest";
import { useFitWindow, type FitPicker } from "./useFitWindow";

function Picker({ compact, fit, rows, spare = 0 }: { compact: boolean; fit: FitPicker; rows: number; spare?: number }) {
  const main = useRef<HTMLElement>(null);
  useFitWindow(main, compact, fit);
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

test("an open menu or dialog asks for the results height even while compact", async () => {
  const fit = vi.fn<FitPicker>(() => Promise.resolve());
  render(<Picker compact fit={fit} rows={0} />);
  const menu = document.createElement("div");
  menu.className = "menu-popover";
  await act(async () => { document.body.append(menu); await Promise.resolve(); });
  expect(fit).toHaveBeenLastCalledWith(null, expect.any(Boolean));
  menu.remove();
});

test("reduced motion resizes without animation", () => {
  const fit = vi.fn<FitPicker>(() => Promise.resolve());
  vi.stubGlobal("matchMedia", (query: string) => ({ matches: query.includes("reduce") }));
  render(<Picker compact={false} fit={fit} rows={1} />);
  expect(fit).toHaveBeenLastCalledWith(null, false);
  vi.unstubAllGlobals();
});
