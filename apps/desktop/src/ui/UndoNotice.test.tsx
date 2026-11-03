import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { useRef, useState } from "react";
import { currentPlatform } from "../platform";
import { NoticeSlot, QuietNotice } from "./NoticeSlot";
import { UndoNotice } from "./UndoNotice";

const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");

afterEach(() => { cleanup(); vi.useRealTimers(); });

test("long action names remain bounded and contain no native-menu control characters", () => {
  render(<UndoNotice message="Removed an invented reference." action={`Remove ${"Invented ".repeat(100)}\u0000\n2018`} onUndo={async () => {}} onExpire={() => {}} />);
  const label = screen.getByRole("button").getAttribute("aria-label")!;
  expect([...label].length).toBeLessThanOrEqual(180);
  expect(label).toContain("2018");
  expect(label).not.toContain("\u0000");
  expect(label).not.toContain("\n");
});

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

test.each([false, true])("pending Undo keeps focus, blocks repeats and respects a newer focus choice (%s)", async (moveFocus) => {
  let finish!: () => void;
  const undo = vi.fn(() => new Promise<void>((resolve) => { finish = resolve; }));
  function Picker() {
    const search = useRef<HTMLInputElement>(null);
    const [visible, setVisible] = useState(true);
    return <><input ref={search} aria-label="Search" /><input aria-label="Page" />
      {visible && <UndoNotice action="Reset Window Position" message="Window position reset." onUndo={async () => { await undo(); setVisible(false); }} onExpire={() => setVisible(false)} fallbackFocusRef={search} />}
    </>;
  }
  render(<Picker />);
  const button = screen.getByRole<HTMLButtonElement>("button", { name: "Undo Reset Window Position" });
  act(() => button.focus());
  fireEvent.click(button);
  await waitFor(() => expect(button.disabled || button.getAttribute("aria-disabled") === "true").toBe(true));
  // Disabling a focused button leaves body focused in Chromium. jsdom even
  // ignores blur() once it is disabled, so model the browser destination.
  if (button.disabled) act(() => {
    document.body.tabIndex = -1;
    document.body.focus();
    document.body.removeAttribute("tabindex");
  });
  expect(document.activeElement === button).toBe(true);
  expect(button.getAttribute("aria-disabled")).toBe("true");
  fireEvent.click(button);
  fireEvent.keyDown(button, { key: "Enter", code: "Enter" });
  fireEvent.keyUp(button, { key: "Enter", code: "Enter" });
  expect(undo).toHaveBeenCalledTimes(1);
  if (moveFocus) act(() => screen.getByRole("textbox", { name: "Page" }).focus());
  await act(async () => { finish(); });
  expect(screen.queryByRole("button", { name: "Undo Reset Window Position" })).toBeNull();
  expect(document.activeElement).toBe(screen.getByRole("textbox", { name: moveFocus ? "Page" : "Search" }));
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

test("failed Undo stays actionable and an in-flight Undo pauses expiry", async () => {
  vi.useFakeTimers();
  let fail!: (error: Error) => void;
  const expire = vi.fn();
  render(<UndoNotice action="Reset Window Position" message="Window position reset." onUndo={() => new Promise((_, reject) => { fail = reject; })} onExpire={expire} />);
  fireEvent.click(screen.getByRole("button", { name: "Undo Reset Window Position" }));
  act(() => vi.advanceTimersByTime(20_000));
  expect(expire).not.toHaveBeenCalled();
  await act(async () => { fail(new Error("synthetic refusal")); });
  expect(screen.getByRole("alert").textContent).toBe("Could not undo. Try again.");
  expect(screen.getByRole<HTMLButtonElement>("button", { name: "Undo Reset Window Position" }).disabled).toBe(false);
});

test("closing and reopening preserves the remaining visible Undo time", () => {
  vi.useFakeTimers();
  const expire = vi.fn();
  const view = (paused: boolean) => <UndoNotice action="Reset Window Position" message="Window position reset." onUndo={async () => {}} onExpire={expire} paused={paused} />;
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

test("a notice stays in the layout on one line and offers its key", () => {
  const { container } = render(<NoticeSlot>
    <UndoNotice action="Reset Window Position" message="Window position reset." onUndo={async () => {}} onExpire={() => {}} />
    <QuietNotice>A remark.</QuietNotice>
  </NoticeSlot>);
  expect(screen.getByRole("region", { name: "Notices" }).children).toHaveLength(2);
  expect(container.querySelector(".notice-band[data-offer]")).not.toBeNull();
  const css = readFileSync(new FileURL("./NoticeSlot.css", import.meta.url), "utf8");
  expect(css).not.toMatch(/position:\s*(fixed|absolute|sticky)|box-shadow|z-index/);
  expect(css).toMatch(/white-space: nowrap/);
  expect(screen.getByRole("button", { name: "Undo Reset Window Position" }).getAttribute("aria-keyshortcuts")).toBeTruthy();
});

test("the newest offer takes the slot and ⌘Z, and an older one returns with the rest of its time", () => {
  vi.useFakeTimers();
  const older = vi.fn(), newer = vi.fn();
  const slot = (both: boolean) => <NoticeSlot>
    <UndoNotice action="Remove Vale 2021" message="Removed Vale 2021" onUndo={async () => {}} onExpire={older} />
    {both && <UndoNotice action="Reset Window Position" message="Window position reset." onUndo={async () => {}} onExpire={newer} />}
  </NoticeSlot>;
  const { rerender } = render(slot(false));
  act(() => vi.advanceTimersByTime(2000));
  rerender(slot(true));
  expect(screen.getByRole("button", { name: "Undo Reset Window Position" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Undo Remove Vale 2021" })).toBeNull();
  act(() => vi.advanceTimersByTime(7000));
  expect(older).not.toHaveBeenCalled();
  rerender(slot(false));
  expect(screen.getByRole("button", { name: "Undo Remove Vale 2021" })).toBeTruthy();
  act(() => vi.advanceTimersByTime(5900));
  expect(older).not.toHaveBeenCalled();
  act(() => vi.advanceTimersByTime(100));
  expect(older).toHaveBeenCalledOnce();
});

test("a remark's time runs only while it is shown, and restarts when it returns", () => {
  vi.useFakeTimers();
  const passed = vi.fn();
  const { rerender } = render(<QuietNotice paused onExpire={passed}>A remark.</QuietNotice>);
  act(() => vi.advanceTimersByTime(9000));
  expect(passed).not.toHaveBeenCalled();
  rerender(<QuietNotice onExpire={passed}>A remark.</QuietNotice>);
  act(() => vi.advanceTimersByTime(7900));
  rerender(<QuietNotice onExpire={() => passed()}>A remark.</QuietNotice>);
  act(() => vi.advanceTimersByTime(100));
  expect(passed).toHaveBeenCalledOnce();
});

test("an empty slot draws nothing", () => {
  const { container } = render(<NoticeSlot>{null}{false}</NoticeSlot>);
  expect(container.firstChild).toBeNull();
});
