import { cleanup, fireEvent, render } from "@testing-library/react";
import { useRef } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { usePickerScroll } from "./usePickerScroll";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

function Picker({ rows, notice, extraNotice, generation = 0 }: { notice?: string; extraNotice?: string; rows: number; generation?: number }) {
  const main = useRef<HTMLElement>(null);
  usePickerScroll(main);
  return <main ref={main}><header className="search-bar"><input id="search" aria-label="Synthetic search" /></header><div className="picker-content" style={{ overflowY: "auto" }}>{notice && <div className="notices"><p role="alert">{notice}</p></div>}{extraNotice && <div className="notices secondary"><p role="alert">{extraNotice}</p></div>}<div className="results-area">{Array.from({ length: rows }, (_, i) => <p role="option" id={`result-${i}`} key={`${generation}/${i}`}>row</p>)}</div></div><footer /></main>;
}

test("a second notice line preserves browsing place when the first notice is unchanged", () => {
  const computed = window.getComputedStyle;
  vi.spyOn(window, "getComputedStyle").mockImplementation((node) => {
    if (!node.matches(".results-area, .picker-content")) return computed(node);
    const style = document.createElement("div").style;
    style.overflowY = node.matches(".picker-content") ? "auto" : "visible";
    return style;
  });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.matches(".notices")) return new DOMRect(0, 53, 720, this.matches(".secondary") ? 40 : 20);
    if (this.matches('[role="option"]')) {
      const middle = this.closest(".picker-content")!;
      return new DOMRect(8, 569 + (middle.querySelector(".secondary") ? 40 : 0) - middle.scrollTop, 704, 58);
    }
    return new DOMRect();
  });
  const view = render(<Picker rows={1} notice="Synthetic settings notice" />);
  const middle = view.container.querySelector<HTMLElement>(".picker-content")!;
  const row = view.container.querySelector('[role="option"]')!;
  middle.scrollTop = 380; fireEvent.scroll(middle);
  view.rerender(<Picker rows={1} notice="Synthetic settings notice" extraNotice="Synthetic size-save notice" />);
  expect(row.getBoundingClientRect().top).toBe(189);
  view.rerender(<Picker rows={1} notice="Synthetic settings notice" />);
  expect(row.getBoundingClientRect().top).toBe(189);
  expect(middle.scrollTop).toBe(380);
});

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
  const view = render(<Picker rows={14} />);
  const results = view.container.querySelector<HTMLElement>(".results-area")!;
  const middle = view.container.querySelector<HTMLElement>(".picker-content")!;
  const row = view.container.querySelector<HTMLElement>("#result-8")!;
  results.scrollTop = 380; fireEvent.scroll(results);
  expect(row.getBoundingClientRect().top).toBe(189);
  view.rerender(<Picker rows={14} notice="Synthetic update progress" generation={replacement ? 1 : 0} />);
  if (replacement) {
    expect(row.isConnected).toBe(false);
    expect(view.container.querySelector("#result-0")!.getBoundingClientRect().top).toBe(131);
    expect([results.scrollTop, middle.scrollTop]).toEqual([0, 0]);
    return;
  }
  expect(row.getBoundingClientRect().top).toBe(189);
  expect([results.scrollTop, middle.scrollTop]).toEqual([0, 422]);
  view.rerender(<Picker rows={14} notice="Expanded synthetic update progress" />);
  expect(row.getBoundingClientRect().top).toBe(189);
  expect([results.scrollTop, middle.scrollTop]).toEqual([0, 464]);
  // Browse elsewhere while the notice is present; clearing must retain that
  // newer place rather than restoring the pre-notice offset.
  middle.scrollTop += 60; fireEvent.scroll(middle);
  expect(row.getBoundingClientRect().top).toBe(129);
  view.rerender(<Picker rows={14} />);
  expect(row.getBoundingClientRect().top).toBe(129);
  expect([results.scrollTop, middle.scrollTop]).toEqual([440, 0]);
});

