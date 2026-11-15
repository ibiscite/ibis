import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { IconButton } from "./IconButton";

test("an icon toggle keeps one outline glyph, reports state and refuses disabled presses", () => {
  const change = vi.fn();
  const view = render(<IconButton icon="pin" aria-label="Keep Window Open" isSelected={false} onChange={change} />);
  const button = screen.getByRole("button", { name: "Keep Window Open" });
  const path = button.querySelector("path")!.getAttribute("d");
  fireEvent.click(button);
  expect(change).toHaveBeenCalledWith(true);
  view.rerender(<IconButton icon="pin" aria-label="Keep Window Open" isSelected onChange={change} isDisabled />);
  expect(button.getAttribute("aria-pressed")).toBe("true");
  expect(button.querySelector("path")!.getAttribute("d")).toBe(path);
  change.mockClear();
  fireEvent.click(button);
  expect(change).not.toHaveBeenCalled();
});
