import { act, render, renderHook, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { useSourceOpen } from "../features/picker/useSourceOpen";
import { SourceAction, shortenedLabel } from "./SourceAction";

afterEach(() => { vi.useRealTimers(); });

test("long names shorten in the middle, keep their extension and name the whole file", () => {
  const label = `Find ${"invented-thesis-references-".repeat(3)}final.bib`;
  expect(shortenedLabel(label).length).toBe(44);
  expect(shortenedLabel(label).endsWith("….bib")).toBe(true);
  expect(shortenedLabel(label).startsWith("Find invented")).toBe(true);
  expect(shortenedLabel("Find invented.bib")).toBe("Find invented.bib");
  render(<SourceAction label={label} icon={null} file open={() => {}} />);
  const action = screen.getByRole("button", { name: label });
  expect(action.getAttribute("title")).toBe(label);
  expect(action.textContent).toBe(shortenedLabel(label));
});

test("a short name has no tooltip, and busy replaces the icon with a spinner", () => {
  const { container } = render(<SourceAction label="Open invented.bib" icon={null} file busy open={() => {}} />);
  const action = screen.getByRole("button", { name: "Open invented.bib" });
  expect(action.getAttribute("title")).toBeNull();
  expect(action.getAttribute("aria-busy")).toBe("true");
  expect(container.querySelector(".source-spinner")).not.toBeNull();
  expect(container.querySelector(".source-icon")).toBeNull();
});

function sourceOpen(open: () => Promise<unknown>) {
  const library = { revision: "1", name: "invented.bib" } as never;
  return renderHook(() => useSourceOpen({ openLibrarySource: open, librarySource: undefined } as never, library, 0, null, false, () => {}));
}

test("a slow open is busy only after 400 ms, and a fast one never is", async () => {
  vi.useFakeTimers();
  let finish!: () => void;
  const slow = sourceOpen(() => new Promise<void>((resolve) => { finish = resolve; }));
  act(() => slow.result.current.run());
  act(() => { vi.advanceTimersByTime(399); });
  expect(slow.result.current.busy).toBe(false);
  act(() => { vi.advanceTimersByTime(1); });
  expect(slow.result.current.busy).toBe(true);
  await act(async () => { finish(); });
  expect(slow.result.current.busy).toBe(false);
  const fast = sourceOpen(() => Promise.resolve());
  await act(async () => { fast.result.current.run(); });
  act(() => { vi.advanceTimersByTime(1000); });
  expect(fast.result.current.busy).toBe(false);
});

test("a failed open stops being busy, and a late first result cannot clear a newer open", async () => {
  vi.useFakeTimers();
  const finish: (() => void)[] = [];
  let reject!: (reason: unknown) => void;
  const hook = sourceOpen(() => new Promise<void>((resolve) => { finish.push(resolve); }));
  act(() => hook.result.current.run());
  act(() => hook.result.current.run());
  act(() => { vi.advanceTimersByTime(400); });
  expect(hook.result.current.busy).toBe(true);
  await act(async () => { finish[0]!(); });
  expect(hook.result.current.busy).toBe(true);
  await act(async () => { finish[1]!(); });
  expect(hook.result.current.busy).toBe(false);
  const failed = sourceOpen(() => new Promise((_, no) => { reject = no; }));
  act(() => failed.result.current.run());
  act(() => { vi.advanceTimersByTime(400); });
  await act(async () => { reject(new Error("invented refusal")); });
  expect(failed.result.current.busy).toBe(false);
  expect(failed.result.current.failure).not.toBeNull();
});
