import { act, cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { HeldKeys, HOLD_MS } from "./HeldKeys";

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal("navigator", { ...navigator, platform: "MacIntel", userAgent: "Macintosh" });
  Element.prototype.getBoundingClientRect = () => ({ left: 10, top: 20, width: 28, height: 28, bottom: 48 }) as DOMRect;
});
afterEach(() => { cleanup(); vi.useRealTimers(); vi.unstubAllGlobals(); });

const scene = () => render(<>
  <button aria-keyshortcuts="Meta+Shift+P">Keep open</button>
  <button aria-keyshortcuts="Meta+K">More<kbd>⌘K</kbd></button>
  <div role="option" aria-keyshortcuts="Meta+1"><span className="result-tile" data-key="⌘1" />Result</div>
  <HeldKeys />
</>);
const held = (container: HTMLElement) => container.querySelectorAll(".ui-held-key");

test("holding the key alone for a second names only controls showing no key", () => {
  const { container } = scene();
  fireEvent.keyDown(window, { key: "Meta" });
  act(() => { vi.advanceTimersByTime(HOLD_MS - 1); });
  expect(held(container)).toHaveLength(0);
  act(() => { vi.advanceTimersByTime(1); });
  expect(held(container)).toHaveLength(1);
  expect(held(container)[0]!.textContent).toContain("P");
  fireEvent.keyUp(window, { key: "Meta" });
  expect(held(container)).toHaveLength(0);
});

test.each([
  ["another key", () => fireEvent.keyDown(window, { key: "k", metaKey: true })],
  ["a click", () => fireEvent.pointerDown(window)],
  ["leaving the window", () => fireEvent.blur(window)],
])("%s cancels a hold in progress", (_, interrupt) => {
  const { container } = scene();
  fireEvent.keyDown(window, { key: "Meta" });
  interrupt();
  act(() => { vi.advanceTimersByTime(HOLD_MS); });
  expect(held(container)).toHaveLength(0);
});

test("closing mid-hold leaves no timer behind", () => {
  const { unmount } = scene();
  fireEvent.keyDown(window, { key: "Meta" });
  unmount();
  expect(vi.getTimerCount()).toBe(0);
});

test("only keys that use the held key are named: a token's ↓ does something else while ⌘ is down", () => {
  const { container } = render(<><button aria-keyshortcuts="ArrowDown">Token</button><button aria-keyshortcuts="Backspace Meta+Shift+F">Format</button><HeldKeys /></>);
  fireEvent.keyDown(window, { key: "Meta" });
  act(() => { vi.advanceTimersByTime(HOLD_MS); });
  expect([...held(container)].map((badge) => badge.textContent)).toEqual(["⇧⌘F"]);
});

test("a badge sits beside its control where it covers nothing, else inside the control's own box", () => {
  // A 28 px control at (100, 100), 20 px badges; `others` are controls already drawn around it.
  const place = (others: Record<string, [number, number]>) => {
    Element.prototype.getBoundingClientRect = function (this: Element) {
      const at = this.textContent === "Keep open" ? [100, 100] : others[this.textContent ?? ""];
      const size = this.classList.contains("ui-held-key") ? 20 : 28;
      const [left, top] = at ?? [0, 0];
      return new DOMRect(left, top, at || this.classList.contains("ui-held-key") ? size : 0, at || this.classList.contains("ui-held-key") ? size : 0);
    };
    const { container, unmount } = render(<><button aria-keyshortcuts="Meta+Shift+P">Keep open</button>{Object.keys(others).map((name) => <button key={name}>{name}</button>)}<HeldKeys /></>);
    fireEvent.keyDown(window, { key: "Meta" });
    act(() => { vi.advanceTimersByTime(HOLD_MS); });
    const badge = held(container)[0] as HTMLElement;
    const found = [badge.dataset.side, badge.style.left, badge.style.top, badge.style.width];
    unmount();
    return found;
  };
  expect(place({})).toEqual(["above", "104px", "76px", ""]);
  expect(place({ Heading: [100, 70] })).toEqual(["below", "104px", "132px", ""]);
  expect(place({ Heading: [100, 70], Rows: [100, 130], Clear: [70, 100], Menu: [130, 100] })).toEqual(["inside", "100px", "100px", "28px"]);
});
