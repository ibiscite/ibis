import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Button, ToggleButton } from "./Button";

afterEach(cleanup);

test("keyboard presses act once and preserve the accessible shortcut", () => {
  const action = vi.fn();
  render(<Button variant="quiet" aria-keyshortcuts="Control+K" onPress={action}>More</Button>);
  const button = screen.getByRole("button", { name: "More" });
  expect(button.getAttribute("aria-keyshortcuts")).toBe("Control+K");
  button.focus();
  fireEvent.keyDown(button, { key: "Enter", code: "Enter" });
  fireEvent.keyUp(button, { key: "Enter", code: "Enter" });
  expect(action).toHaveBeenCalledTimes(1);
});

test("disabled actions cannot activate, including synthetic clicks", () => {
  const action = vi.fn();
  render(<Button isDisabled onPress={action}>Copy</Button>);
  const button = screen.getByRole("button", { name: "Copy" });
  fireEvent.click(button);
  fireEvent.keyDown(button, { key: "Enter" });
  expect(action).not.toHaveBeenCalled();
  expect(button.hasAttribute("data-disabled")).toBe(true);
});

test("a controlled toggle reports its next state without changing its label", () => {
  const change = vi.fn();
  const { rerender } = render(<ToggleButton variant="icon" isSelected={false} onChange={change} aria-label="Keep window open">↟</ToggleButton>);
  const button = screen.getByRole("button", { name: "Keep window open" });
  fireEvent.click(button);
  expect(change).toHaveBeenCalledWith(true);
  rerender(<ToggleButton variant="icon" isSelected onChange={change} aria-label="Keep window open">↟</ToggleButton>);
  expect(button.getAttribute("aria-pressed")).toBe("true");
  expect(button.hasAttribute("data-selected")).toBe(true);
});

test("folded actions keep their hidden state and leave the tab order", () => {
  const { rerender } = render(<Button aria-hidden inert excludeFromTabOrder>Shortcuts</Button>);
  expect(screen.queryByRole("button", { name: "Shortcuts" })).toBeNull();
  const button = screen.getByRole("button", { hidden: true });
  expect(button.tabIndex).toBe(-1);
  rerender(<Button>Shortcuts</Button>);
  expect(screen.getByRole("button", { name: "Shortcuts" }).tabIndex).toBe(0);
});
