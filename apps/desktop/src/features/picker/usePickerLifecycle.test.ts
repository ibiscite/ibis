import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type * as nativeApi from "../../api";
import type { LibraryStatus, PickerSnapshot } from "../../bindings";
import { useLibraryStatus } from "./useLibraryStatus";
import { usePickerLifecycle } from "./usePickerLifecycle";

afterEach(cleanup);

type Lifecycle = Parameters<typeof usePickerLifecycle>[0];
const status = (name: string) => ({ name, state: "loaded", revision: name }) as unknown as LibraryStatus;
const snapshot = (presentation: number, library: LibraryStatus) =>
  ({ presentation, selection: null, library, destination: { Err: null }, preferences: { Err: null }, recents: { Err: null } }) as unknown as PickerSnapshot;

test("an ignored presentation leaves a status read in flight to finish", async () => {
  let resolveRead: (value: LibraryStatus) => void = () => undefined;
  let prepare: ((value: PickerSnapshot) => void) | undefined;
  const api = {
    libraryStatus: () => new Promise<LibraryStatus>((resolve) => { resolveRead = resolve; }),
    pickerSnapshot: async () => null,
    pickerRendered: async () => undefined,
    onPickerPreparing: (handler: (value: PickerSnapshot) => void) => { prepare = handler; return Promise.resolve(() => undefined); },
  } as unknown as typeof nativeApi;
  const others = {
    pickerSession: { session: { getSnapshot: () => ({ permissionDetour: false, changing: null }) }, send: vi.fn() },
    menus: { input: { current: null }, focusHome: vi.fn() },
    destination: { snapshotReceiver: () => vi.fn(), read: vi.fn(), writingChanged: vi.fn() },
    preferences: { snapshotReceiver: () => vi.fn(), preferencesChanged: vi.fn() },
    recents: { snapshotReceiver: () => vi.fn() },
  } as unknown as Pick<Lifecycle, "pickerSession" | "menus" | "destination" | "preferences" | "recents">;
  const report = vi.fn();
  const resetPointer = () => undefined;
  const { result } = renderHook(() => {
    const libraryState = useLibraryStatus(api, report, vi.fn());
    usePickerLifecycle({ api, ...others, libraryState: libraryState as Lifecycle["libraryState"], resetPointer, readShortcut: vi.fn(), report, setAltHeld: vi.fn() });
    return libraryState;
  });
  await act(async () => { await Promise.resolve(); });
  act(() => prepare?.(snapshot(1, status("a"))));
  expect(result.current.library).toEqual(status("a"));
  act(() => result.current.refreshSnapshot());
  // The same presentation again is ignored, so it must not retire the read.
  act(() => prepare?.(snapshot(1, status("a"))));
  await act(async () => { resolveRead(status("b")); await Promise.resolve(); });
  expect(result.current.library).toEqual(status("b"));
});
