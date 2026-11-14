import { act, cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { HeldKeys, HOLD_MS, KeyMark } from "./HeldKeys";
import { Button } from "./Button";

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal("navigator", { ...navigator, platform: "MacIntel", userAgent: "Macintosh" });
});
afterEach(() => { cleanup(); vi.useRealTimers(); vi.unstubAllGlobals(); });

const held = () => document.documentElement.hasAttribute("data-keys-held");

test("holding the key alone for a second shows the marks, and letting go hides them", () => {
  render(<HeldKeys />);
  fireEvent.keyDown(window, { key: "Meta" });
  act(() => { vi.advanceTimersByTime(HOLD_MS - 1); });
  expect(held()).toBe(false);
  act(() => { vi.advanceTimersByTime(1); });
  expect(held()).toBe(true);
  fireEvent.keyUp(window, { key: "Meta" });
  expect(held()).toBe(false);
});

test.each([
  ["another key", () => fireEvent.keyDown(window, { key: "k", metaKey: true })],
  ["a click", () => fireEvent.pointerDown(window)],
  ["leaving the window", () => fireEvent.blur(window)],
])("%s cancels a hold in progress", (_, interrupt) => {
  render(<HeldKeys />);
  fireEvent.keyDown(window, { key: "Meta" });
  interrupt();
  act(() => { vi.advanceTimersByTime(HOLD_MS); });
  expect(held()).toBe(false);
});

test("closing mid-hold leaves no timer behind", () => {
  const { unmount } = render(<HeldKeys />);
  fireEvent.keyDown(window, { key: "Meta" });
  unmount();
  expect(vi.getTimerCount()).toBe(0);
});

test("a hold marks rows with digits alone and keeps a control's words", () => {
  const { container } = render(<><HeldKeys /><KeyMark keys="CmdOrCtrl+1" /><Button label="Format" keys="CmdOrCtrl+Shift+F" /></>);
  fireEvent.keyDown(window, { key: "Meta" });
  act(() => { vi.advanceTimersByTime(HOLD_MS); });
  expect(held()).toBe(true);
  expect([...container.querySelectorAll(".ui-key-mark")].map((mark) => mark.textContent)).toEqual(["1"]);
  expect(container.querySelector("button")?.textContent).toContain("Format");
});
