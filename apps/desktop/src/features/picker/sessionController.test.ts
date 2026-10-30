import { expect, test, vi } from "vitest";
import type { Copied } from "../../api";
import { testStates } from "../../test-fixtures";
import { choiceKey } from "./choice";
import { sessionQueryKey } from "./session";
import { createSessionController, type SessionPorts } from "./sessionController";

function setup(overrides: Partial<SessionPorts> = {}) {
  const controller = createSessionController();
  const ports: SessionPorts = { copyFileCitation: vi.fn(async () => ({ text: "canonical citation", warnings: [] })),
    hidePicker: vi.fn(async () => {}), showCopiedNotice: vi.fn(async () => ({ outcome: "pasted" as const, app: "Example Writer" })),
    output: () => ({ opening: 0, choice: 0 }), pinned: () => false, consume: vi.fn(), refresh: vi.fn(), askFormat: vi.fn(), reportHide: vi.fn(), ...overrides };
  controller.connect(ports);
  const chosen = [testStates[1]!.results[0]!];
  const ready = () => {
    const state = controller.getSnapshot();
    controller.dispatch({ type: "view", session: state.session, interaction: state.interaction,
      view: { queryKey: sessionQueryKey(state), format: "pandoc", snapshotRevision: "synthetic", defaultChoice: choiceKey(chosen), chosen,
        shown: { text: "canonical citation", html: "", markup: [], warnings: [], fileTicket: "prepared", fileHash: "hash" },
        searchPending: false, ready: true, saving: false, shouldAsk: false, output: ports.output() } });
  };
  ready();
  const enter = () => controller.dispatch({ type: "enter", choice: choiceKey(chosen) });
  return { controller, ports, ready, enter };
}
const deferred = <T,>() => {
  let resolve!: (value: T) => void, reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};

test("the copy result carries the first three hints across close/reopen without another operation", async () => {
  let count = 0;
  const copyFileCitation = vi.fn(async () => ({ text: "canonical citation", warnings: [], paste_hint: ++count <= 3 }));
  const s = setup({ pinned: () => true, copyFileCitation });
  for (let i = 1; i <= 4; i++) {
    s.ready(); s.enter(); s.enter();
    await vi.waitFor(() => expect(s.controller.getSnapshot().copied?.paste_hint).toBe(i <= 3));
    expect(s.controller.getSnapshot().lastOutcome).toBe(i <= 3 ? "Copied. Press Ctrl+V to paste" : "Copied");
    s.controller.dispatch({ type: "hidden", now: 10 });
    s.controller.dispatch({ type: "shown", now: 20 });
  }
  expect(copyFileCitation).toHaveBeenCalledTimes(4);
  const reopened = setup({ pinned: () => true, copyFileCitation });
  reopened.enter();
  await vi.waitFor(() => expect(reopened.controller.getSnapshot().copied?.paste_hint).toBe(false));
});

test.each([
  [null, "Copied. Press Ctrl+V to paste"],
  ["terminal", "Copied. Not pasted into a terminal"],
  ["unsupported-system", "Copied. Paste it into your document"],
] as const)("a native receipt with reason %s keeps its meaning with the original copy hint", async (reason, title) => {
  const s = setup({ copyFileCitation: vi.fn(async () => ({ text: "canonical citation", warnings: [], paste_hint: true })),
    showCopiedNotice: vi.fn(async () => ({ outcome: "copied" as const, reason })) });
  s.enter();
  await vi.waitFor(() => expect(s.controller.getSnapshot().lastOutcome).toBe(title));
  expect(s.ports.showCopiedNotice).toHaveBeenCalledExactlyOnceWith("canonical citation", null, true);
});

test("warnings keep their meaning even if a hint is supplied", async () => {
  const s = setup({ pinned: () => true,
    copyFileCitation: vi.fn(async () => ({ text: "canonical citation", warnings: ["Invented warning"], paste_hint: true })) });
  s.enter();
  await vi.waitFor(() => expect(s.controller.getSnapshot().copied?.warnings).toEqual(["Invented warning"]));
  expect(s.controller.getSnapshot().lastOutcome).toBe("Copied");
});

test("delayed copy A cannot overwrite B's hint or start another feedback operation", async () => {
  const old = deferred<Copied>();
  const s = setup({ pinned: () => true, copyFileCitation: vi.fn().mockReturnValueOnce(old.promise)
    .mockResolvedValue({ text: "new citation", warnings: [], paste_hint: false }) });
  s.enter();
  s.controller.dispatch({ type: "hidden", now: 10 });
  s.controller.dispatch({ type: "shown", now: 20 });
  s.ready(); s.enter();
  await vi.waitFor(() => expect(s.controller.getSnapshot().copied?.paste_hint).toBe(false));
  old.resolve({ text: "old citation", warnings: [], paste_hint: true });
  await old.promise;
  expect(s.ports.copyFileCitation).toHaveBeenCalledTimes(2);
  expect(s.ports.showCopiedNotice).not.toHaveBeenCalled();
  expect(s.controller.getSnapshot().copied?.text).toBe("new citation");
  expect(s.controller.getSnapshot().lastOutcome).toBe("Copied");
});

test("the runner writes and delivers one canonical result despite repeated Enter", async () => {
  const pending = deferred<Copied>();
  const s = setup({ copyFileCitation: vi.fn(() => pending.promise) });
  s.enter(); s.enter();
  expect(s.ports.copyFileCitation).toHaveBeenCalledExactlyOnceWith("prepared", "hash");
  pending.resolve({ text: "canonical citation", warnings: [], delivery: 42 });
  await pending.promise;
  await Promise.resolve();
  expect(s.ports.showCopiedNotice).toHaveBeenCalledExactlyOnceWith("canonical citation", 42, null);
  await Promise.resolve();
  expect(s.controller.getSnapshot().lastOutcome).toBe("Pasted into Example Writer");
  expect(s.ports.consume).toHaveBeenCalledTimes(1);
});

test("an old copy response cannot consume a reopened session's new copy", async () => {
  const old = deferred<Copied>(), fresh = deferred<Copied>();
  const s = setup({ copyFileCitation: vi.fn().mockReturnValueOnce(old.promise).mockReturnValueOnce(fresh.promise) });
  s.enter();
  s.controller.dispatch({ type: "hidden", now: 10 });
  s.controller.dispatch({ type: "shown", now: 20 });
  s.ready(); s.enter();
  old.resolve({ text: "old citation", warnings: [] });
  await old.promise;
  expect(s.ports.consume).not.toHaveBeenCalled();
  expect(s.ports.hidePicker).not.toHaveBeenCalled();
  expect(s.controller.getSnapshot().activeCopy).toBe(2);
  fresh.resolve({ text: "fresh citation", warnings: [] });
  await fresh.promise;
  await Promise.resolve();
  expect(s.ports.showCopiedNotice).toHaveBeenCalledExactlyOnceWith("fresh citation", null, null);
});

test("a delayed hide rejection cannot show an error in a reopened session", async () => {
  const hiding = deferred<void>();
  const s = setup({ hidePicker: vi.fn(() => hiding.promise) });
  s.enter();
  await Promise.resolve();
  s.controller.dispatch({ type: "hidden", now: 10 });
  s.controller.dispatch({ type: "shown", now: 20 });
  hiding.reject(new Error("invented hide failure"));
  await hiding.promise.catch(() => {});
  expect(s.ports.reportHide).not.toHaveBeenCalled();
  expect(s.ports.consume).not.toHaveBeenCalled();
  expect(s.ports.showCopiedNotice).not.toHaveBeenCalled();
});

test.each([false, true])("a rejected copy shows a fixed error and only stale input refreshes (stale=%s)", async (stale) => {
  const s = setup({ copyFileCitation: vi.fn().mockRejectedValue(stale ? { code: "snapshot_changed" } : new Error("invented private payload")) });
  s.enter();
  await Promise.resolve();
  expect(s.controller.getSnapshot().copyError).toBe(stale
    ? "The library changed. Search again and reselect the references before copying." : "Nothing was copied. Try again, or use Reload.");
  expect(s.ports.refresh).toHaveBeenCalledTimes(stale ? 1 : 0);
  expect(s.ports.hidePicker).not.toHaveBeenCalled();
  expect(s.ports.showCopiedNotice).not.toHaveBeenCalled();
});
