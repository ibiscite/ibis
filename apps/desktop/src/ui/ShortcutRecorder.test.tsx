import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { ShortcutRecorder } from "./ShortcutRecorder";

function recorder() {
  const record = vi.fn();
  render(<ShortcutRecorder shortcut="Command+Shift+Y" label="Open Ibis" prompt="Type a shortcut…" isDisabled={false} onRecord={record} />);
  const button = screen.getByRole("button", { name: /^Open Ibis:/ });
  fireEvent.click(button);
  expect(button.textContent).toBe("Type a shortcut…");
  return { button, record };
}

test("the recorder uses the typed ASCII letter and a stable modifier order", () => {
  const { button, record } = recorder();
  const allowed = fireEvent.keyDown(button, { key: "a", code: "KeyQ", ctrlKey: true, altKey: true, shiftKey: true, metaKey: true });
  expect(allowed).toBe(false);
  expect(record).toHaveBeenCalledExactlyOnceWith("Control+Alt+Shift+Command+A");
  expect(button.getAttribute("aria-pressed")).toBe("false");
});

test.each([
  { key: "й", code: "KeyQ", expected: "KeyQ" },
  { key: "Dead", code: "KeyE", expected: "KeyE" },
  { key: "!", code: "Digit1", expected: "Digit1" },
  { key: " ", code: "Space", expected: "Space" },
  { key: "F8", code: "F8", expected: "F8" },
])("$key records the physical code", ({ key, code, expected }) => {
  const { button, record } = recorder();
  fireEvent.keyDown(button, { key, code, altKey: true });
  expect(record).toHaveBeenCalledExactlyOnceWith(`Alt+${expected}`);
});

test("modifiers, repeated keys and composition wait for a complete press", () => {
  const { button, record } = recorder();
  for (const key of ["Control", "Alt", "Shift", "Meta", "AltGraph"]) fireEvent.keyDown(button, { key, code: `${key}Left` });
  fireEvent.keyDown(button, { key: "y", code: "KeyY", repeat: true });
  fireEvent.keyDown(button, { key: "y", code: "KeyY", isComposing: true });
  fireEvent.keyDown(button, { key: "Process", code: "KeyY", keyCode: 229 });
  expect(record).not.toHaveBeenCalled();
  expect(button.textContent).toBe("Type a shortcut…");
  fireEvent.keyDown(button, { key: "y", code: "KeyY", altKey: true });
  expect(record).toHaveBeenCalledExactlyOnceWith("Alt+Y");
});

test.each(["Escape", "blur"])("%s cancels without changing the shortcut", (cancel) => {
  const { button, record } = recorder();
  if (cancel === "blur") { button.focus(); fireEvent.blur(button); }
  else fireEvent.keyDown(button, { key: "Escape", code: "Escape" });
  expect(record).not.toHaveBeenCalled();
  expect(button.textContent).not.toBe("Type a shortcut…");
  fireEvent.keyDown(button, { key: "k", code: "KeyK", altKey: true });
  expect(record).not.toHaveBeenCalled();
});
