import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Button } from "./Button";

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
