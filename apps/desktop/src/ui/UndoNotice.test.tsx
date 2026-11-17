import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { currentPlatform } from "../platform";
import { UndoNotice } from "./UndoNotice";

afterEach(() => { cleanup(); vi.useRealTimers(); });

test("the notice and Edit menu share an action label and Command-Z executes it once", async () => {
  const undo = vi.fn(async () => {});
  const { rerender, unmount } = render(<UndoNotice message="Removed from citation." action="Remove Okafor & Lima, 2018" onUndo={undo} onExpire={() => {}} />);
  const button = screen.getByRole("button", { name: "Undo Remove Okafor & Lima, 2018" });
  expect(button.getAttribute("aria-keyshortcuts")).toContain("Z");
  await act(async () => fireEvent.keyDown(window, { key: "z", metaKey: currentPlatform() === "macos", ctrlKey: currentPlatform() !== "macos" }));
  expect(undo).toHaveBeenCalledOnce();
  rerender(<UndoNotice message="Look reset." action="Reset Look" onUndo={undo} onExpire={() => {}} paused />);
  await act(async () => fireEvent.keyDown(window, { key: "z", metaKey: currentPlatform() === "macos", ctrlKey: currentPlatform() !== "macos" }));
  expect(undo).toHaveBeenCalledOnce();
  unmount();
  await act(async () => fireEvent.keyDown(window, { key: "z", metaKey: currentPlatform() === "macos", ctrlKey: currentPlatform() !== "macos" }));
  expect(undo).toHaveBeenCalledOnce();
});

test("Undo expires after eight visible seconds and pauses on hover and focus", () => {
  vi.useFakeTimers();
  const expire = vi.fn();
  render(<><input aria-label="Elsewhere" /><UndoNotice action="Reset Window Position" message="Window position reset." onUndo={async () => {}} onExpire={expire} /></>);
  act(() => vi.advanceTimersByTime(7000));
  fireEvent.pointerEnter(screen.getByText("Window position reset.").parentElement!);
  act(() => vi.advanceTimersByTime(20_000));
  expect(expire).not.toHaveBeenCalled();
  act(() => screen.getByRole("button", { name: "Undo Reset Window Position" }).focus());
  fireEvent.pointerLeave(screen.getByText("Window position reset.").parentElement!);
  act(() => vi.advanceTimersByTime(20_000));
  expect(expire).not.toHaveBeenCalled();
  act(() => screen.getByLabelText("Elsewhere").focus());
  act(() => vi.advanceTimersByTime(999));
  expect(expire).not.toHaveBeenCalled();
  act(() => vi.advanceTimersByTime(1));
  expect(expire).toHaveBeenCalledOnce();
});
