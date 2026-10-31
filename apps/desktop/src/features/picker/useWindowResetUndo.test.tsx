import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { PickerApi } from "../../App";
import { useWindowResetUndo } from "./useWindowResetUndo";

afterEach(cleanup);

test.each([true, false])("old reset reads, action completions (%s) and expiry cannot consume a newer ticket", async (restored) => {
  let changed!: (state: { revision: number; undo: number | null }) => void;
  let read!: (value: number | null) => void, finish!: (value: boolean) => void;
  const restore = vi.fn(() => new Promise<boolean>((resolve) => { finish = resolve; }));
  const dismiss = vi.fn(async () => {});
  let actions!: ReturnType<typeof useWindowResetUndo>;
  const api = { readWindowPositionUndo: () => new Promise<number | null>((resolve) => { read = resolve; }),
    onWindowReset: async (handler: typeof changed) => { changed = handler; return () => {}; },
    undoWindowPosition: restore, dismissWindowPositionUndo: dismiss } as unknown as PickerApi;
  function Picker() { actions = useWindowResetUndo(api); return <p>{actions.ticket ?? "none"}</p>; }
  render(<Picker />);
  await act(async () => {});
  act(() => changed({ revision: 2, undo: 2 }));
  await act(async () => { read(1); });
  expect(screen.getByText("2")).toBeTruthy();
  const old = actions;
  let pending!: Promise<void>;
  act(() => { pending = old.undo(); });
  act(() => changed({ revision: 3, undo: 3 }));
  await act(async () => { finish(restored); await pending; });
  expect(screen.getByText("3")).toBeTruthy();
  act(() => old.expire());
  expect(dismiss).not.toHaveBeenCalled();
  await act(async () => { await old.undo(); });
  expect(restore).toHaveBeenCalledTimes(1);
  act(() => actions.expire());
  expect(screen.getByText("none")).toBeTruthy();
  expect(dismiss).toHaveBeenCalledExactlyOnceWith(3);
  act(() => changed({ revision: 3, undo: 3 }));
  expect(screen.getByText("none")).toBeTruthy();
});

test("a delayed reset event cannot replace a newer ticket read from the native owner", async () => {
  let changed!: (state: { revision: number; undo: number | null }) => void;
  let actions!: ReturnType<typeof useWindowResetUndo>;
  const api = {
    readWindowPositionUndo: async () => 5,
    onWindowReset: async (handler: typeof changed) => { changed = handler; return () => {}; },
  } as unknown as PickerApi;
  function Picker() { actions = useWindowResetUndo(api); return <p>{actions.ticket ?? "none"}</p>; }
  render(<Picker />);
  await act(async () => {});
  expect(screen.getByText("5")).toBeTruthy();
  act(() => changed({ revision: 4, undo: 4 }));
  expect(screen.getByText("5")).toBeTruthy();
  act(() => changed({ revision: 6, undo: null }));
  expect(screen.getByText("none")).toBeTruthy();
});
