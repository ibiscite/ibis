import { act, cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { HeldKeys, HOLD_MS } from "./HeldKeys";

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal("navigator", { ...navigator, platform: "MacIntel", userAgent: "Macintosh" });
  Element.prototype.getBoundingClientRect = () => ({ left: 10, top: 20, width: 28, height: 28 }) as DOMRect;
});
afterEach(() => { cleanup(); vi.useRealTimers(); vi.unstubAllGlobals(); });

const scene = () => render(<>
  <button aria-keyshortcuts="Meta+Shift+P">Keep open</button>
  <button aria-keyshortcuts="Meta+K">More<kbd>⌘K</kbd></button>
  <HeldKeys />
</>);
const held = (container: HTMLElement) => container.querySelectorAll(".held-key");

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
