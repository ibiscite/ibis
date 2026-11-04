import { expect, test, vi } from "vitest";
import type { PickerPreferences, PickerSave } from "./bindings";
import { PreferencePatchQueue, type PreferenceRead } from "./preferencePatchQueue";

const defaults: PickerPreferences = { output: "pandoc", density: "comfortable", larger_text: false, paste: true, menu_bar: true };
const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  let reject!: (error?: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};

function fixture(policy = {}) {
  let state: PickerSave = { preferences: { ...defaults } };
  let issued = 0;
  const api = {
    revision: vi.fn(async () => ++issued),
    read: vi.fn(async (): Promise<PreferenceRead> => state),
    write: vi.fn(async (patch: Partial<PickerPreferences>) => {
      state = { preferences: { ...state.preferences, ...patch } };
      return state;
    }),
  };
  const queue = new PreferencePatchQueue({ read: api.read, revision: api.revision, write: (patch) => api.write(patch) }, policy);
  const events = vi.fn<Parameters<typeof queue.subscribe>[0]>();
  queue.subscribe(events);
  return { api, queue, events, saved: () => state };
}

test("revisions are requested when the person acts, even behind an unfinished save", async () => {
  let issued = 0;
  const first = deferred<PickerSave>();
  const api = {
    revision: vi.fn(async () => ++issued),
    read: async () => ({ preferences: defaults }),
    write: vi.fn(async (_patch: Partial<PickerPreferences>, _revision: number) => first.promise),
  };
  const queue = new PreferencePatchQueue(api);
  await queue.load();
  queue.change({ density: "compact" });
  queue.change({ density: "compact" });
  expect(api.revision).toHaveBeenCalledTimes(2);
  first.resolve({ preferences: { ...defaults, density: "compact" } });
  await queue.finishSaving();
  expect(api.write.mock.calls.map((call) => call[1])).toEqual([1, 2]);
});

test("a rejected ticket does not prevent the next choice from saving", async () => {
  const { api, queue, saved } = fixture();
  await queue.load();
  api.revision.mockRejectedValueOnce(new Error("synthetic issuance failure"));
  queue.change({ larger_text: true });
  await queue.finishSaving();
  expect(api.write).not.toHaveBeenCalled();
  queue.change({ density: "compact" });
  await queue.finishSaving();
  expect(api.write.mock.calls).toEqual([[{ density: "compact" }]]);
  expect(saved().preferences).toMatchObject({ density: "compact", larger_text: false });
});

test("retrying one field keeps its old ticket when a newer choice saves another field", async () => {
  let issued = 0;
  let disk = { ...defaults };
  let attempt = 0;
  const tickets: number[] = [];
  const queue = new PreferencePatchQueue({
    revision: async () => ++issued,
    read: async () => ({ preferences: disk }),
    write: async (patch, ticket) => {
      tickets.push(ticket);
      if (++attempt === 1) throw new Error("synthetic write failure");
      // The other window has already written larger_text with ticket 2.
      if (ticket > 2 || patch.larger_text === undefined) disk = { ...disk, ...patch };
      return { preferences: disk };
    },
  });
  await queue.load();
  queue.change({ larger_text: true });
  await queue.finishSaving();
  issued += 1;
  disk = { ...disk, larger_text: false };
  queue.change({ density: "compact" });
  await queue.finishSaving();
  expect(tickets).toEqual([1, 1, 3]);
  expect(queue.savedPreferences).toMatchObject({ density: "compact", larger_text: false });
});

test.each([false, true])("writes stay ordered and acknowledged fields never ride a later patch (readAfterWrites=%s)", async (readAfterWrites) => {
  const { api, queue, events } = fixture({ readAfterWrites });
  await queue.load();
  const first = deferred<PickerSave>();
  api.write.mockReturnValueOnce(first.promise);
  queue.change({ output: "cite" });
  queue.change({ larger_text: true });
  await vi.waitFor(() => expect(api.write).toHaveBeenCalledTimes(1));
  expect(api.write).toHaveBeenCalledWith({ output: "cite" });
  // Another window chooses a style after the first write acknowledges.
  first.resolve({ preferences: defaults });
  api.write.mockResolvedValueOnce({ preferences: { ...defaults, output: "ieee", larger_text: true } });
  await queue.finishSaving();
  expect(api.write.mock.calls[1]).toEqual([{ larger_text: true }]);
  expect(events).toHaveBeenLastCalledWith({ type: "saved", value: { preferences: { ...defaults, output: "ieee", larger_text: true } } });
});

test("equal-valued later intents keep their own revision through failure and retry", async () => {
  const { api, queue, saved } = fixture();
  await queue.load();
  const first = deferred<PickerSave>();
  api.write.mockReturnValueOnce(first.promise);
  api.write.mockImplementationOnce(async () => ({ preferences: defaults }));
  api.write.mockRejectedValueOnce(new Error("synthetic save failure"));
  queue.change({ larger_text: true });
  queue.change({ larger_text: false });
  queue.change({ larger_text: true });
  queue.change({ density: "compact" });
  first.resolve({ preferences: { ...defaults, larger_text: true } });
  await queue.finishSaving();
  expect(api.write.mock.calls).toEqual([
    [{ larger_text: true }], [{ larger_text: false }],
    [{ larger_text: true }], [{ larger_text: true }], [{ density: "compact" }],
  ]);
  expect(saved().preferences).toMatchObject({ larger_text: true, density: "compact" });
});

test("Settings shortcut writes share ordering and closing waits with preference writes", async () => {
  const { api, queue } = fixture();
  await queue.load();
  const first = deferred<PickerSave>(), shortcut = deferred<void>(), started = deferred<void>();
  api.write.mockReturnValueOnce(first.promise);
  queue.change({ density: "compact" });
  const shortcutSave = queue.enqueue(() => { started.resolve(); return shortcut.promise; });
  queue.change({ larger_text: true });
  const closed = vi.fn();
  const closing = queue.finishSaving().then(closed);
  first.resolve({ preferences: { ...defaults, density: "compact" } });
  await started.promise;
  expect(api.write).toHaveBeenCalledTimes(1);
  expect(closed).not.toHaveBeenCalled();
  shortcut.resolve();
  await shortcutSave;
  await closing;
  expect(api.write).toHaveBeenLastCalledWith({ larger_text: true });
  expect(closed).toHaveBeenCalledOnce();
});

test("an unreadable initial file holds explicit edits until a successful read", async () => {
  const { api, queue, saved } = fixture({ readAfterWrites: true });
  api.read.mockResolvedValueOnce({ preferences: null });
  queue.change({ output: "cite" });
  queue.change({ paste: false });
  await queue.load();
  expect(api.write).not.toHaveBeenCalled();
  await queue.load();
  await queue.finishSaving();
  expect(api.write.mock.calls).toEqual([[{ output: "cite" }], [{ paste: false }]]);
  expect(api.revision).toHaveBeenCalledTimes(2);
  expect(saved()).toMatchObject({ preferences: { paste: false } });
});

test.each([false, true])("the latest read supersedes older read success and failure (readAfterWrites=%s)", async (readAfterWrites) => {
  const { api, queue, events } = fixture({ readAfterWrites });
  const old = deferred<PreferenceRead>(), latest = deferred<PreferenceRead>();
  api.read.mockReturnValueOnce(old.promise).mockReturnValueOnce(latest.promise);
  const first = queue.load(), second = queue.load();
  latest.resolve({ preferences: { ...defaults, larger_text: true } });
  await second;
  old.resolve({ preferences: defaults });
  await first;
  expect(events).toHaveBeenCalledTimes(1);
  const failed = deferred<PreferenceRead>();
  api.read.mockReturnValueOnce(failed.promise);
  const stale = queue.load();
  await queue.load();
  events.mockClear();
  failed.reject(new Error("synthetic stale read failure"));
  await stale;
  expect(events).not.toHaveBeenCalled();
});

test("Settings recovery restores durable state, but cannot overwrite a newer edit", async () => {
  const { api, queue, events, saved } = fixture({ restoreOnFailure: true });
  await queue.load();
  const recovery = deferred<PreferenceRead>();
  api.read.mockReturnValueOnce(recovery.promise);
  api.write.mockRejectedValueOnce(new Error("synthetic save failure"));
  queue.change({ larger_text: true });
  await queue.finishSaving();
  expect(api.read).toHaveBeenCalledTimes(2);
  queue.change({ density: "compact" });
  await queue.finishSaving();
  expect(saved().preferences).toMatchObject({ larger_text: true, density: "compact" });
  events.mockClear();
  recovery.resolve({ preferences: defaults });
  await recovery.promise;
  expect(events).not.toHaveBeenCalled();

  api.write.mockRejectedValueOnce(new Error("synthetic save failure"));
  queue.change({ larger_text: false });
  await queue.finishSaving();
  expect(events).toHaveBeenLastCalledWith({ type: "read", value: saved(), pendingOutput: false });
  queue.change({ paste: false });
  await queue.finishSaving();
  expect(api.write).toHaveBeenLastCalledWith({ paste: false });
});

test("Settings focus restores saved choices after a failed recovery read and keeps the edit retryable", async () => {
  const { api, queue, events, saved } = fixture({ restoreOnFailure: true });
  await queue.load();
  api.write.mockRejectedValueOnce(new Error("synthetic save failure"));
  api.read.mockRejectedValueOnce(new Error("synthetic recovery failure"));
  queue.change({ larger_text: true });
  await queue.finishSaving();
  await queue.load();
  expect(events).toHaveBeenLastCalledWith({ type: "read", value: saved(), pendingOutput: false });
  queue.change({ density: "compact" });
  await queue.finishSaving();
  expect(api.write.mock.calls.slice(-2)).toEqual([[{ larger_text: true }], [{ density: "compact" }]]);
});

test("a native change notification waits for local writes and refreshes cross-window state", async () => {
  let issued = 0;
  const revision = async () => ++issued;
  let state: PickerSave = { preferences: { ...defaults } };
  const local = deferred<void>();
  const read = vi.fn(async () => state);
  const write = async (patch: Partial<PickerPreferences>) => {
    if (patch.density) await local.promise;
    state = { preferences: { ...state.preferences, ...patch } };
    // Native preferences:changed is emitted before the write command returns.
    void picker.load();
    return state;
  };
  const picker = new PreferencePatchQueue({ read, write, revision }, { readAfterWrites: true });
  const settings = new PreferencePatchQueue({ read: async () => state, write, revision }, { restoreOnFailure: true });
  const updates = vi.fn<Parameters<typeof picker.subscribe>[0]>();
  const stop = picker.subscribe(updates);
  await Promise.all([picker.load(), settings.load()]);
  picker.change({ density: "compact" });
  settings.change({ larger_text: true });
  await settings.finishSaving();
  // The other window's notification cannot read behind the local write.
  expect(read).toHaveBeenCalledOnce();
  local.resolve();
  await picker.finishSaving();
  expect(updates).toHaveBeenLastCalledWith({ type: "read", value: state, pendingOutput: false });
  expect(state.preferences).toMatchObject({ density: "compact", larger_text: true });
  // Settings gets the picker's change on focus, through the same read guard.
  const settingsUpdates = vi.fn<Parameters<typeof settings.subscribe>[0]>();
  settings.subscribe(settingsUpdates);
  await settings.load();
  expect(settingsUpdates).toHaveBeenLastCalledWith({ type: "read", value: state, pendingOutput: false });
  stop();
  updates.mockClear();
  await picker.load();
  expect(updates).not.toHaveBeenCalled();
});

test.each([false, true])("notifications retire old replies before an unbounded refresh (readAfterWrites=%s)", async (readAfterWrites) => {
  const { api, queue, events } = fixture({ readAfterWrites });
  await queue.load();
  const write = deferred<PickerSave>(), read = deferred<PreferenceRead>();
  api.write.mockReturnValueOnce(write.promise);
  api.read.mockReturnValueOnce(read.promise);
  queue.change({ density: "compact" });
  const refresh = queue.refresh(2);
  events.mockClear();
  vi.useFakeTimers();
  try {
    await vi.advanceTimersByTimeAsync(2_500);
    expect(api.read).toHaveBeenCalledOnce();
    write.resolve({ preferences: { ...defaults, density: "compact" } });
    await vi.advanceTimersByTimeAsync(0);
    expect(api.read).toHaveBeenCalledTimes(2);
    expect(events).not.toHaveBeenCalled();
    const latest = { preferences: { ...defaults, density: "compact" as const } };
    read.resolve(latest);
    await refresh;
    expect(events).toHaveBeenCalledExactlyOnceWith({ type: "read", value: latest, pendingOutput: false });
    expect(queue.savedPreferences?.density).toBe("compact");
  } finally { vi.useRealTimers(); }
});

test.each(["before reply", "after reply"])("an own notification %s keeps the last successful rollback value", async (when) => {
  const { api, queue } = fixture({ restoreOnFailure: true });
  await queue.load();
  const saved = { preferences: { ...defaults, density: "compact" as const } };
  api.read.mockRejectedValue(new Error("synthetic read failure"));
  api.write.mockImplementationOnce(async () => {
    if (when === "before reply") void queue.refresh(1);
    return saved;
  }).mockRejectedValueOnce(new Error("synthetic second write failure"));
  queue.change({ density: "compact" });
  if (when === "after reply") { await queue.finishSaving(); void queue.refresh(1); }
  queue.change({ larger_text: true });
  await queue.finishSaving();
  expect(queue.savedPreferences).toMatchObject({ density: "compact", larger_text: false });
});

test("a foreign notification still retires a late acknowledgement followed by its own event", async () => {
  const { api, queue } = fixture({ restoreOnFailure: true });
  await queue.load();
  const saving = deferred<PickerSave>(), reading = deferred<PreferenceRead>();
  api.write.mockReturnValueOnce(saving.promise);
  api.read.mockReturnValueOnce(reading.promise);
  queue.change({ density: "compact" });
  await vi.waitFor(() => expect(api.write).toHaveBeenCalledOnce());
  const foreign = queue.refresh(2);
  const own = queue.refresh(1);
  saving.resolve({ preferences: { ...defaults, density: "compact" } });
  await vi.waitFor(() => expect(api.read).toHaveBeenCalledTimes(2));
  expect(queue.savedPreferences).toBeNull();
  reading.resolve({ preferences: { ...defaults, density: "compact" } });
  await Promise.all([foreign, own]);
  expect(queue.savedPreferences?.density).toBe("compact");
});

test("a failed read publishes nothing after unmount", async () => {
  const { api, queue, events } = fixture();
  const read = deferred<PreferenceRead>();
  api.read.mockReturnValueOnce(read.promise);
  const loading = queue.load(() => false);
  read.reject(new Error("synthetic unavailable read"));
  await loading;
  expect(events).not.toHaveBeenCalled();
});
