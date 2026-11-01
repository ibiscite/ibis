import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { IconButton } from "./IconButton";
import { SettingsSearch } from "./SettingsSearch";

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

test("Settings search clear is named, clears once and returns focus to typing", () => {
  const change = vi.fn();
  const view = render(<SettingsSearch label="Search settings" value="invented" onChange={change} />);
  const clear = screen.getByRole("button", { name: "Clear Search" });
  fireEvent.click(clear);
  expect(change).toHaveBeenCalledExactlyOnceWith("");
  expect(document.activeElement).toBe(screen.getByRole("searchbox"));
  view.rerender(<SettingsSearch label="Search settings" value="" onChange={change} />);
  expect(screen.queryByRole("button", { name: "Clear Search" })).toBeNull();
});
