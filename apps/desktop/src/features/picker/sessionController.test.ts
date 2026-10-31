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
    output: () => ({ opening: 0, choice: 0, delivery: 0 }), pinned: () => false, consume: vi.fn(), refresh: vi.fn(), askFormat: vi.fn(), reportHide: vi.fn(), ...overrides };
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

test("the runner writes and delivers one canonical result despite repeated Enter", async () => {
  const pending = deferred<Copied>();
  const s = setup({ copyFileCitation: vi.fn(() => pending.promise) });
  s.enter(); s.enter();
  expect(s.ports.copyFileCitation).toHaveBeenCalledExactlyOnceWith("prepared", "hash");
  pending.resolve({ text: "canonical citation", warnings: [], delivery: 42 });
  await pending.promise;
  await Promise.resolve();
  expect(s.ports.showCopiedNotice).toHaveBeenCalledExactlyOnceWith("canonical citation", 42);
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
  expect(s.ports.showCopiedNotice).toHaveBeenCalledExactlyOnceWith("fresh citation", null);
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
