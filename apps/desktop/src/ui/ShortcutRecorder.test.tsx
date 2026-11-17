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

test.each(["Escape", "blur"])("%s cancels without changing the shortcut", (cancel) => {
  const { button, record } = recorder();
  if (cancel === "blur") { button.focus(); fireEvent.blur(button); }
  else fireEvent.keyDown(button, { key: "Escape", code: "Escape" });
  expect(record).not.toHaveBeenCalled();
  expect(button.textContent).not.toBe("Type a shortcut…");
  fireEvent.keyDown(button, { key: "k", code: "KeyK", altKey: true });
  expect(record).not.toHaveBeenCalled();
});
