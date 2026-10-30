import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { UndoNotice } from "./UndoNotice";

afterEach(() => { cleanup(); vi.useRealTimers(); });

test("Undo expires after eight visible seconds and pauses on hover and focus", () => {
  vi.useFakeTimers();
  const expire = vi.fn();
  render(<><input aria-label="Elsewhere" /><UndoNotice message="Window position reset." onUndo={async () => {}} onExpire={expire} /></>);
  act(() => vi.advanceTimersByTime(7000));
  fireEvent.pointerEnter(screen.getByText("Window position reset.").parentElement!);
  act(() => vi.advanceTimersByTime(20_000));
  expect(expire).not.toHaveBeenCalled();
  act(() => screen.getByRole("button", { name: "Undo" }).focus());
  fireEvent.pointerLeave(screen.getByText("Window position reset.").parentElement!);
  act(() => vi.advanceTimersByTime(20_000));
  expect(expire).not.toHaveBeenCalled();
  act(() => screen.getByLabelText("Elsewhere").focus());
  act(() => vi.advanceTimersByTime(999));
  expect(expire).not.toHaveBeenCalled();
  act(() => vi.advanceTimersByTime(1));
  expect(expire).toHaveBeenCalledOnce();
});

test("failed Undo stays actionable and an in-flight Undo pauses expiry", async () => {
  vi.useFakeTimers();
  let fail!: (error: Error) => void;
  const expire = vi.fn();
  render(<UndoNotice message="Window position reset." onUndo={() => new Promise((_, reject) => { fail = reject; })} onExpire={expire} />);
  fireEvent.click(screen.getByRole("button", { name: "Undo" }));
  act(() => vi.advanceTimersByTime(20_000));
  expect(expire).not.toHaveBeenCalled();
  await act(async () => { fail(new Error("synthetic refusal")); });
  expect(screen.getByRole("alert").textContent).toBe("Could not undo. Try again.");
  expect(screen.getByRole<HTMLButtonElement>("button", { name: "Undo" }).disabled).toBe(false);
});

test("closing and reopening preserves the remaining visible Undo time", () => {
  vi.useFakeTimers();
  const expire = vi.fn();
  const view = (paused: boolean) => <UndoNotice message="Window position reset." onUndo={async () => {}} onExpire={expire} paused={paused} />;
  const { rerender } = render(view(false));
  act(() => vi.advanceTimersByTime(3000));
  rerender(view(true));
  act(() => vi.advanceTimersByTime(60_000));
  expect(expire).not.toHaveBeenCalled();
  rerender(view(false));
  act(() => vi.advanceTimersByTime(4999));
  expect(expire).not.toHaveBeenCalled();
  act(() => vi.advanceTimersByTime(1));
  expect(expire).toHaveBeenCalledOnce();
});
