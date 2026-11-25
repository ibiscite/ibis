import { act, cleanup, renderHook } from "@testing-library/react";
import { StrictMode } from "react";
import { afterEach, expect, test, vi } from "vitest";
import type * as nativeApi from "../../api";
import type { LibraryState, LibraryStatus } from "../../bindings";
import { useLibraryStatus } from "./useLibraryStatus";

afterEach(cleanup);

const status = (name: string, state: LibraryState = "loaded") => ({ name, state, revision: name }) as unknown as LibraryStatus;

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

/** A native event a test can fire once its handler is listening. */
function source<T>() {
  const handlers = new Set<(payload: T) => void>();
  return {
    listen: (handler: (payload: T) => void) => { handlers.add(handler); return Promise.resolve(() => { handlers.delete(handler); }); },
    emit: (payload: T) => act(() => { for (const handler of handlers) handler(payload); }),
  };
}

async function setup(strict = false) {
  const reads: ReturnType<typeof deferred<LibraryStatus>>[] = [];
  const changed = source<LibraryStatus>();
  const hidden = source<null>();
  const api = {
    libraryStatus: vi.fn(() => { const read = deferred<LibraryStatus>(); reads.push(read); return read.promise; }),
    onLibraryChanged: changed.listen,
    onPickerHidden: hidden.listen,
  } as unknown as typeof nativeApi;
  const report = vi.fn();
  const cancelCopy = vi.fn();
  const hook = renderHook(() => useLibraryStatus(api, report, cancelCopy), strict ? { wrapper: StrictMode } : undefined);
  await act(async () => { await Promise.resolve(); });
  const settle = (index: number, result: LibraryStatus | Error) => act(async () => {
    const read = reads[index];
    if (result instanceof Error) read?.reject(result); else read?.resolve(result);
    await Promise.resolve();
  });
  return { hook, current: () => hook.result.current, changed, hidden, report, cancelCopy, settle };
}

test("a read publishes the library and clears its failure", async () => {
  const t = await setup();
  expect(t.current().library).toBeUndefined();
  act(() => t.current().refreshSnapshot());
  await t.settle(0, status("a"));
  expect(t.current().library).toEqual(status("a"));
  expect(t.current().settledLibrary).toEqual(status("a"));
  expect(t.current().libraryVersion).toBe(1);
  expect(t.report).toHaveBeenLastCalledWith("status", false);
});

test("a failed first read shows no library, cancels the copy and reports once", async () => {
  const t = await setup();
  act(() => t.current().refreshSnapshot());
  await t.settle(0, new Error("invented failure"));
  expect(t.current().library).toBeNull();
  expect(t.current().settledLibrary).toBeNull();
  expect(t.cancelCopy).toHaveBeenCalledTimes(1);
  expect(t.report).toHaveBeenCalledWith("status");
});

test("a failed read keeps the library already shown", async () => {
  const t = await setup();
  await t.changed.emit(status("a"));
  act(() => t.current().refreshSnapshot());
  await t.settle(0, new Error("invented failure"));
  expect(t.current().library).toEqual(status("a"));
});

test("a newer read replaces an older one that finishes late", async () => {
  const t = await setup();
  act(() => t.current().refreshSnapshot());
  act(() => t.current().refreshSnapshot());
  await t.settle(1, status("b"));
  await t.settle(0, status("a"));
  expect(t.current().library).toEqual(status("b"));
  expect(t.current().libraryVersion).toBe(1);
});

test("an event retires the read before it, success or failure", async () => {
  const t = await setup();
  act(() => t.current().refreshSnapshot());
  act(() => t.current().refreshSnapshot());
  await t.changed.emit(status("event"));
  t.report.mockClear();
  await t.settle(0, status("late"));
  await t.settle(1, new Error("invented failure"));
  expect(t.current().library).toEqual(status("event"));
  expect(t.cancelCopy).not.toHaveBeenCalled();
  expect(t.report).not.toHaveBeenCalled();
});

test("hiding retires a pending read", async () => {
  const t = await setup();
  act(() => t.current().refreshSnapshot());
  await t.hidden.emit(null);
  await t.settle(0, new Error("invented failure"));
  expect(t.current().library).toBeUndefined();
  expect(t.cancelCopy).not.toHaveBeenCalled();
});

test("a reload keeps the settled library and its version until it ends", async () => {
  const t = await setup();
  await t.changed.emit(status("a"));
  await t.changed.emit(status("a", "reloading"));
  expect(t.current().library?.state).toBe("reloading");
  expect(t.current().settledLibrary).toEqual(status("a"));
  expect(t.current().libraryVersion).toBe(1);
  await t.changed.emit(status("b"));
  expect(t.current().settledLibrary).toEqual(status("b"));
  expect(t.current().libraryVersion).toBe(2);
});

test("a first status that is a reload settles on itself", async () => {
  const t = await setup();
  await t.changed.emit(status("a", "reloading"));
  expect(t.current().settledLibrary).toEqual(status("a", "reloading"));
  expect(t.current().libraryVersion).toBe(0);
});

test("a picker snapshot is accepted while nothing newer arrived", async () => {
  const t = await setup();
  const receive = t.current().snapshotReceiver();
  act(() => receive(status("snapshot", "reloading")));
  expect(t.current().library).toEqual(status("snapshot", "reloading"));
  expect(t.current().settledLibrary).toEqual(status("snapshot", "reloading"));
  expect(t.current().libraryVersion).toBe(1);
  expect(t.report).toHaveBeenLastCalledWith("status", false);
});

test("a snapshot reserved before an event, read or hide is ignored", async () => {
  const t = await setup();
  const beforeEvent = t.current().snapshotReceiver();
  await t.changed.emit(status("event"));
  act(() => beforeEvent(status("old")));
  const beforeRead = t.current().snapshotReceiver();
  act(() => t.current().refreshSnapshot());
  act(() => beforeRead(status("old")));
  await t.settle(0, status("read"));
  const beforeHide = t.current().snapshotReceiver();
  await t.hidden.emit(null);
  act(() => beforeHide(status("old")));
  expect(t.current().library).toEqual(status("read"));
});

test("an accepted snapshot retires the read before it", async () => {
  const t = await setup();
  act(() => t.current().refreshSnapshot());
  const receive = t.current().snapshotReceiver();
  act(() => receive(status("snapshot")));
  await t.settle(0, status("late"));
  expect(t.current().library).toEqual(status("snapshot"));
});

test("leaving retires a pending read", async () => {
  const t = await setup();
  act(() => t.current().refreshSnapshot());
  t.hook.unmount();
  await t.settle(0, new Error("invented failure"));
  expect(t.cancelCopy).not.toHaveBeenCalled();
  expect(t.report).not.toHaveBeenCalled();
});

test("a snapshot reserved after a read retires that read, even before the snapshot arrives", async () => {
  const t = await setup();
  act(() => t.current().refreshSnapshot());
  const receive = t.current().snapshotReceiver();
  await t.settle(0, status("read"));
  expect(t.current().library).toBeUndefined();
  act(() => receive(status("snapshot")));
  expect(t.current().library).toEqual(status("snapshot"));
});

test("the owner survives StrictMode's second mount", async () => {
  const t = await setup(true);
  await t.changed.emit(status("event"));
  act(() => t.current().refreshSnapshot());
  await t.settle(0, status("read"));
  expect(t.current().library).toEqual(status("read"));
  expect(t.current().libraryVersion).toBe(2);
});
