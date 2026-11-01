import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { AboutDelight, ShortcutEcho, useKeyPulse } from "./Delights";
import { currentPlatform } from "../platform";
import { KeyCaps } from "./KeyCaps";

afterEach(() => vi.useRealTimers());

test("footer caps pulse for keyboard shortcuts only and retire old timers", () => {
  vi.useFakeTimers();
  function Footer() { const pressed = useKeyPulse("Enter"); return <div data-echo={pressed || undefined}><KeyCaps accelerator="Enter" /></div>; }
  const { container, unmount } = render(<Footer />);
  fireEvent.click(container.firstChild!);
  expect(container.querySelector("[data-echo]")).toBeNull();
  fireEvent.keyDown(window, { key: "Enter" });
  expect(container.querySelector("[data-echo]")).not.toBeNull();
  act(() => vi.advanceTimersByTime(80));
  fireEvent.keyDown(window, { key: "Enter" });
  act(() => vi.advanceTimersByTime(20));
  expect(container.querySelector("[data-echo]")).not.toBeNull();
  act(() => vi.advanceTimersByTime(80));
  expect(container.querySelector("[data-echo]")).toBeNull();
  fireEvent.keyDown(window, { key: "Enter", isComposing: true });
  expect(container.querySelector("[data-echo]")).toBeNull();
  unmount(); render(<Footer />);
  expect(container.querySelector("[data-echo]")).toBeNull();
});

test("About renders only the authorized blink and clicking never changes focus", () => {
  const click = vi.fn();
  const { container, rerender } = render(<AboutDelight blink={1} enabled={false} onClick={click} />);
  expect(container.querySelector('[data-blink]')).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Ibis" }));
  expect(click).toHaveBeenCalledOnce();
  rerender(<AboutDelight blink={1} enabled onClick={click} />);
  expect(container.querySelector('[data-blink]')).not.toBeNull();
});

test("listed shortcut echo is quiet and refuses recording or composition", () => {
  vi.useFakeTimers();
  const { container } = render(<ShortcutEcho keys={["CmdOrCtrl+Shift+P"]}><span>Keep open</span></ShortcutEcho>);
  fireEvent.keyDown(window, { key: "P", metaKey: currentPlatform() === "macos", ctrlKey: currentPlatform() !== "macos", shiftKey: true });
  expect(container.querySelector("[data-echo]")).not.toBeNull();
  act(() => vi.advanceTimersByTime(100));
  expect(container.querySelector("[data-echo]")).toBeNull();
  const recorder = document.createElement("span"); recorder.dataset.recording = "true"; document.body.append(recorder);
  fireEvent.keyDown(recorder, { key: "P", metaKey: currentPlatform() === "macos", ctrlKey: currentPlatform() !== "macos", shiftKey: true });
  expect(container.querySelector("[data-echo]")).toBeNull(); recorder.remove();
});

test.each([
  ["CommandOrControl+Shift+Y", { key: "Y", shiftKey: true, metaKey: true }],
  ["Control+Y", { key: "Y", ctrlKey: true }],
  ["Command+Y", { key: "Y", metaKey: true }],
  ["Space", { key: " " }],
  ["?", { key: "?", shiftKey: true }],
  ["CmdOrCtrl+/", { key: "?", metaKey: true, shiftKey: true }],
] as const)("listed %s echoes its actual recorded keys", (accelerator, press) => {
  const system = document.documentElement.dataset.system;
  document.documentElement.dataset.system = "macos";
  try {
    const { container } = render(<ShortcutEcho keys={[accelerator]}><span>Invented shortcut</span></ShortcutEcho>);
    fireEvent.keyDown(window, press);
    expect(container.querySelector("[data-echo]")).not.toBeNull();
  } finally { if (system === undefined) delete document.documentElement.dataset.system; else document.documentElement.dataset.system = system; }
});
