import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Button, ToggleButton } from "./Button";
import { createRef } from "react";
import { ToggleButtonContext } from "react-aria-components";

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


test("shortcut updates preserve consumer and context refs on the shared toggle", () => {
  const consumer = createRef<HTMLButtonElement>(), context = createRef<HTMLButtonElement>();
  const change = vi.fn();
  const control = (shortcut?: string) => <ToggleButtonContext.Provider value={{ ref: context }}>
    <ToggleButton variant="icon" ref={consumer} aria-label="Keep Window Open" aria-keyshortcuts={shortcut} isSelected onChange={change}>Pin</ToggleButton>
  </ToggleButtonContext.Provider>;
  const view = render(control("Control+Shift+P"));
  const button = screen.getByRole("button", { name: "Keep Window Open" });
  view.rerender(control("Meta+Shift+P"));
  expect(button.getAttribute("aria-keyshortcuts")).toBe("Meta+Shift+P");
  view.rerender(control());
  expect(button.hasAttribute("aria-keyshortcuts")).toBe(false);
  expect(consumer.current).toBe(button); expect(context.current).toBe(button);
  fireEvent.click(button);
  expect(change).toHaveBeenCalledWith(false);
  expect(button.getAttribute("aria-pressed")).toBe("true");
  view.unmount();
  expect(consumer.current).toBeNull(); expect(context.current).toBeNull();
});
