import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { IconButton } from "./IconButton";
import { Switch } from "./Switch";

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

test("a control drawn without words cannot be built without a name", () => {
  // Checked by the compiler (tsc): each line below fails to type-check unless it is named.
  // @ts-expect-error A glyph-only button needs aria-label or aria-labelledby.
  render(<IconButton icon="pin" />);
  // @ts-expect-error So does a switch, whose label sits elsewhere in its row.
  render(<Switch />);
  render(<><IconButton icon="pin" aria-label="Keep Window Open" /><Switch aria-labelledby="row" /></>);
  expect(screen.getAllByRole("button").filter((node) => node.getAttribute("aria-label")).length).toBeGreaterThan(0);
});
