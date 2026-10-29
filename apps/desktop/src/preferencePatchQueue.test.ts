import { expect, test, vi } from "vitest";
import type { KeyFormat, PickerPreferences, PickerSave } from "./bindings";
import { PreferencePatchQueue, type PreferenceRead } from "./preferencePatchQueue";

const defaults: PickerPreferences = { citation_style: null, density: "comfortable", larger_text: false, show_paste_preview: true, search_mark: false, paste: true };
const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  let reject!: (error?: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};

function fixture(policy = {}) {
  let state: PickerSave = { preferences: { ...defaults }, copy_format: "pandoc" };
  const api = {
    read: vi.fn(async (): Promise<PreferenceRead> => state),
    write: vi.fn(async (patch: Partial<PickerPreferences>, key: KeyFormat | null) => {
      state = { preferences: { ...state.preferences, ...patch }, copy_format: key ?? state.copy_format };
      return state;
    }),
  };
  const queue = new PreferencePatchQueue(api, policy);
  const events = vi.fn<Parameters<typeof queue.subscribe>[0]>();
  queue.subscribe(events);
  return { api, queue, events, saved: () => state };
}

test.each([false, true])("writes stay ordered and acknowledged fields never ride a later patch (readAfterWrites=%s)", async (readAfterWrites) => {
  const { api, queue, events } = fixture({ readAfterWrites });
  await queue.load();
  const first = deferred<PickerSave>();
  api.write.mockReturnValueOnce(first.promise);
  queue.change({ citation_style: null }, "cite");
  queue.change({ larger_text: true });
  await Promise.resolve();
  await Promise.resolve();
  expect(api.write).toHaveBeenCalledTimes(1);
  expect(api.write).toHaveBeenCalledWith({ citation_style: null }, "cite");
  // Another window chooses a style after the first write acknowledges.
  first.resolve({ preferences: defaults, copy_format: "cite" });
  api.write.mockResolvedValueOnce({ preferences: { ...defaults, citation_style: "ieee", larger_text: true }, copy_format: "cite" });
  await queue.finishSaving();
  expect(api.write.mock.calls[1]).toEqual([{ larger_text: true }, null]);
  expect(events).toHaveBeenLastCalledWith({ type: "saved", value: { preferences: { ...defaults, citation_style: "ieee", larger_text: true }, copy_format: "cite" } });
});

test("equal-valued later intents keep their own revision through failure and retry", async () => {
  const { api, queue, saved } = fixture();
  await queue.load();
  const first = deferred<PickerSave>();
  api.write.mockReturnValueOnce(first.promise);
  api.write.mockImplementationOnce(async () => ({ preferences: defaults, copy_format: "pandoc" }));
  api.write.mockRejectedValueOnce(new Error("synthetic save failure"));
  queue.change({ larger_text: true });
  queue.change({ larger_text: false });
  queue.change({ larger_text: true });
  queue.change({ density: "spacious" });
  first.resolve({ preferences: { ...defaults, larger_text: true }, copy_format: "pandoc" });
  await queue.finishSaving();
  expect(api.write.mock.calls).toEqual([
    [{ larger_text: true }, null], [{ larger_text: false }, null],
    [{ larger_text: true }, null], [{ larger_text: true, density: "spacious" }, null],
  ]);
  expect(saved().preferences).toMatchObject({ larger_text: true, density: "spacious" });
});

test("Settings shortcut writes share ordering and closing waits with preference writes", async () => {
  const { api, queue } = fixture();
  await queue.load();
  const first = deferred<PickerSave>(), shortcut = deferred<void>(), started = deferred<void>();
  api.write.mockReturnValueOnce(first.promise);
  queue.change({ density: "spacious" });
  const shortcutSave = queue.enqueue(() => { started.resolve(); return shortcut.promise; });
  queue.change({ larger_text: true });
  const closed = vi.fn();
  const closing = queue.finishSaving().then(closed);
  first.resolve({ preferences: { ...defaults, density: "spacious" }, copy_format: "pandoc" });
  await started.promise;
  expect(api.write).toHaveBeenCalledTimes(1);
  expect(closed).not.toHaveBeenCalled();
  shortcut.resolve();
  await shortcutSave;
  await closing;
  expect(api.write).toHaveBeenLastCalledWith({ larger_text: true }, null);
  expect(closed).toHaveBeenCalledOnce();
});

test("an unreadable initial file holds explicit edits until a successful read", async () => {
  const { api, queue, saved } = fixture({ readAfterWrites: true });
  api.read.mockResolvedValueOnce({ preferences: null, copy_format: "typst" });
  queue.change({ citation_style: null }, "cite");
  queue.change({ show_paste_preview: false });
  await queue.load();
  expect(api.write).not.toHaveBeenCalled();
  await queue.load();
  await queue.finishSaving();
  expect(api.write.mock.calls).toEqual([[{ citation_style: null, show_paste_preview: false }, "cite"]]);
  expect(saved()).toMatchObject({ preferences: { show_paste_preview: false }, copy_format: "cite" });
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
  queue.change({ density: "spacious" });
  await queue.finishSaving();
  expect(saved().preferences).toMatchObject({ larger_text: true, density: "spacious" });
  events.mockClear();
  recovery.resolve({ preferences: defaults });
  await recovery.promise;
  expect(events).not.toHaveBeenCalled();

  api.write.mockRejectedValueOnce(new Error("synthetic save failure"));
  queue.change({ larger_text: false });
  await queue.finishSaving();
  expect(events).toHaveBeenLastCalledWith({ type: "read", value: saved(), pendingStyle: false });
  queue.change({ search_mark: true });
  await queue.finishSaving();
  expect(api.write).toHaveBeenLastCalledWith({ search_mark: true }, null);
});

test("Settings focus restores saved choices after a failed recovery read and keeps the edit retryable", async () => {
  const { api, queue, events, saved } = fixture({ restoreOnFailure: true });
  await queue.load();
  api.write.mockRejectedValueOnce(new Error("synthetic save failure"));
  api.read.mockRejectedValueOnce(new Error("synthetic recovery failure"));
  queue.change({ larger_text: true });
  await queue.finishSaving();
  await queue.load();
  expect(events).toHaveBeenLastCalledWith({ type: "read", value: saved(), pendingStyle: false });
  queue.change({ density: "spacious" });
  await queue.finishSaving();
  expect(api.write).toHaveBeenLastCalledWith({ larger_text: true, density: "spacious" }, null);
});

test("a native change notification waits for local writes and refreshes cross-window state", async () => {
  let state: PickerSave = { preferences: { ...defaults }, copy_format: "pandoc" };
  const local = deferred<void>();
  const read = vi.fn(async () => state);
  const write = async (patch: Partial<PickerPreferences>, key: KeyFormat | null) => {
    if (patch.density) await local.promise;
    state = { preferences: { ...state.preferences, ...patch }, copy_format: key ?? state.copy_format };
    // Native preferences:changed is emitted before the write command returns.
    void picker.load();
    return state;
  };
  const picker = new PreferencePatchQueue({ read, write }, { readAfterWrites: true });
  const settings = new PreferencePatchQueue({ read: async () => state, write }, { restoreOnFailure: true });
  const updates = vi.fn<Parameters<typeof picker.subscribe>[0]>();
  const stop = picker.subscribe(updates);
  await Promise.all([picker.load(), settings.load()]);
  picker.change({ density: "spacious" });
  settings.change({ larger_text: true });
  await settings.finishSaving();
  // The other window's notification cannot read behind the local write.
  expect(read).toHaveBeenCalledOnce();
  local.resolve();
  await picker.finishSaving();
  expect(updates).toHaveBeenLastCalledWith({ type: "read", value: state, pendingStyle: false });
  expect(state.preferences).toMatchObject({ density: "spacious", larger_text: true });
  // Settings gets the picker's change on focus, through the same read guard.
  const settingsUpdates = vi.fn<Parameters<typeof settings.subscribe>[0]>();
  settings.subscribe(settingsUpdates);
  await settings.load();
  expect(settingsUpdates).toHaveBeenLastCalledWith({ type: "read", value: state, pendingStyle: false });
  stop();
  updates.mockClear();
  await picker.load();
  expect(updates).not.toHaveBeenCalled();
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
