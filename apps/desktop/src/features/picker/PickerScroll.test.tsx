import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { PickerScroll } from "./PickerScroll";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

function Picker({ rows, notice, extraNotice, generation = 0 }: { notice?: string; extraNotice?: string; rows: number; generation?: number }) {
  return <main><header className="search-bar"><input id="search" aria-label="Synthetic search" /></header><PickerScroll>{notice && <div className="notices"><p role="alert">{notice}</p></div>}{extraNotice && <div className="notices secondary"><p role="alert">{extraNotice}</p></div>}<div className="results-area">{Array.from({ length: rows }, (_, i) => <p role="option" id={`result-${i}`} key={`${generation}/${i}`}>row</p>)}</div></PickerScroll><footer /></main>;
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

test.each([
  { replacement: false, delayedScroll: false, middleOnly: false },
  { replacement: false, delayedScroll: true, middleOnly: false },
  { replacement: false, delayedScroll: false, middleOnly: true },
  { replacement: false, delayedScroll: true, middleOnly: true },
  { replacement: true, delayedScroll: false, middleOnly: false },
])("notice appear and clear preserve only the current browsing anchor (%j)", ({ replacement, delayedScroll, middleOnly }) => {
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
    style.overflowY = node.matches(".results-area") && (middleOnly || hasNotice(node as HTMLElement)) ? "visible" : "auto";
    return style;
  });
  vi.spyOn(HTMLElement.prototype, "scrollTop", "get").mockImplementation(function (this: HTMLElement) {
    // Browsers clamp an offset when its element stops being a scrollport.
    if (this.matches(".results-area") && (middleOnly || hasNotice(this)) || this.matches(".picker-content") && !middleOnly && !hasNotice(this)) scrollTop.set!.call(this, 0);
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
  // At the minimum frame the middle owns scrolling before and after the notice;
  // the default frame transfers ownership to it only while a notice is present.
  const initialOwner = middleOnly ? middle : results;
  initialOwner.scrollTop = 380; fireEvent.scroll(initialOwner);
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
  middle.scrollTop += 60;
  // A notice can clear after the person scrolls but before the browser delivers
  // that scroll event. The DOM's current place still owns the next commit.
  if (!delayedScroll) fireEvent.scroll(middle);
  expect(row.getBoundingClientRect().top).toBe(129);
  view.rerender(<Picker rows={14} />);
  expect(row.getBoundingClientRect().top).toBe(129);
  expect([results.scrollTop, middle.scrollTop]).toEqual(middleOnly ? [0, 440] : [440, 0]);
  fireEvent.scroll(middle);
  fireEvent.scroll(results);
  expect(row.getBoundingClientRect().top).toBe(129);
});

