import { act, cleanup, render } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { QuietNotice } from "./NoticeSlot";

afterEach(() => { cleanup(); vi.useRealTimers(); });

test("a remark keeps its remaining visible time across hides and starts fresh for a different notice", () => {
  vi.useFakeTimers();
  const expire = vi.fn();
  const view = (paused: boolean, message = "Invented remark") => <QuietNotice onExpire={expire} paused={paused}>{message}</QuietNotice>;
  const { rerender } = render(view(false));
  act(() => vi.advanceTimersByTime(3000));
  rerender(view(true));
  act(() => vi.advanceTimersByTime(20000));
  rerender(view(false));
  act(() => vi.advanceTimersByTime(4999));
  expect(expire).not.toHaveBeenCalled();
  act(() => vi.advanceTimersByTime(1));
  expect(expire).toHaveBeenCalledOnce();
  rerender(view(true, "Another invented remark"));
  act(() => vi.advanceTimersByTime(20000));
  rerender(view(false, "Another invented remark"));
  act(() => vi.advanceTimersByTime(7999));
  expect(expire).toHaveBeenCalledOnce();
  act(() => vi.advanceTimersByTime(1));
  expect(expire).toHaveBeenCalledTimes(2);
});
