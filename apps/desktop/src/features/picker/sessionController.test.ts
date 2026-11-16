import { expect, test, vi } from "vitest";
import type { DeliveryOutcome } from "../../bindings";
import { testStates } from "../../test-fixtures";
import { choiceKey } from "./choice";
import { arrangeView } from "../../testing/sessionView";
import { initialSession, sessionQueryKey, type SessionState } from "./session";
import { createSessionController, type SessionPorts } from "./sessionController";
import { deferred } from "../../testing/deferred";

function setup(overrides: Partial<SessionPorts> = {}) {
  const chosen = [testStates[1]!.results[0]!];
  const [arranged] = arrangeView({ ...initialSession(), presentation: 0 }, { queryKey: sessionQueryKey(initialSession()), format: "apa", snapshotRevision: "synthetic",
    defaultChoice: choiceKey(chosen), offered: chosen, chosen, shown: { display: { nodes: [] }, text: "canonical citation", warnings: [], fileTicket: "prepared", fileHash: "hash" },
    searchPending: false, ready: true, saving: false, shouldAsk: false, remembersLast: true, output: { opening: 0, choice: 0, delivery: 0 } });
  // A format picked for this citation only: the copy that uses it consumes it.
  const controller = createSessionController({ ...arranged, destination: { ...arranged.destination, held: "apa" } });
  const ports: SessionPorts = { pickerPresentation: vi.fn(async () => 0),
    deliverCitation: vi.fn(async (): Promise<DeliveryOutcome> => ({ outcome: "pasted", copied: { text: "canonical citation", warnings: [] }, app: "Example Writer" })),
    refresh: vi.fn(), report: vi.fn(), chooseDefault: vi.fn(), search: vi.fn(async () => ({ revision: "synthetic", results: [] })), previewFileCitation: vi.fn(async () => { throw new Error("not asked"); }), ...overrides };
  controller.connect(ports);
  /** The picker tells a new opening what it shows: the settled search and preview are adopted. */
  const ready = () => {
    const { inputs, search, preview } = controller.getSnapshot();
    controller.dispatch({ type: "picker-inputs", inputs: { ...inputs! } });
    controller.dispatch({ type: "search-inputs", inputs: search.inputs! });
    controller.dispatch({ type: "preview-inputs", inputs: preview.inputs! });
    controller.dispatch({ type: "committed" });
  };
  ready();
  const enter = () => controller.dispatch({ type: "enter", choice: choiceKey(chosen) });
  return { controller, ports, ready, enter };
}
const held = (state: SessionState) => state.destination.held;

const copied = { text: "canonical citation", warnings: [] };

test("repeated Enter submits one command with the prepared ticket and native presentation", async () => {
  const pending = deferred<DeliveryOutcome>();
  const s = setup({ deliverCitation: vi.fn(() => pending.promise) });
  s.enter(); s.enter();
  expect(s.ports.deliverCitation).toHaveBeenCalledExactlyOnceWith("prepared", "hash", null, 0, []);
  pending.resolve({ outcome: "pasted", copied, app: "Example Writer" });
  await pending.promise;
  expect(s.controller.getSnapshot().lastOutcome).toBe("Pasted into Example Writer");
  expect(held(s.controller.getSnapshot())).toBeNull();
  expect(s.controller.getSnapshot().flights).toHaveLength(0);
});

test.each([
  { outcome: "pasted", copied, app: "Example Writer" },
  { outcome: "copied-only", copied, reason: "copy-only" },
  { outcome: "retained", copied, reason: "pinned" },
  { outcome: "sent", copied, retained: null },
] satisfies DeliveryOutcome[])("the picker confirms $outcome and consumes its held format", async (outcome) => {
  const s = setup({ deliverCitation: vi.fn(async () => outcome) });
  s.enter(); await Promise.resolve();
  expect(s.controller.getSnapshot().copied).toEqual(copied);
  expect(held(s.controller.getSnapshot())).toBeNull();
  expect(s.controller.getSnapshot().copyError).toBeNull();
  if (outcome.outcome === "copied-only" && outcome.reason === "copy-only") {
    expect(s.controller.getSnapshot().lastOutcome).toContain("Copy Only is set for this app");
  }
});

test("native hiding may arrive before the single delivery response", async () => {
  const pending = deferred<DeliveryOutcome>();
  const s = setup({ deliverCitation: vi.fn(() => pending.promise) });
  s.enter();
  s.controller.dispatch({ type: "hidden", now: 10 });
  pending.resolve({ outcome: "pasted", copied, app: "Example Writer" });
  await pending.promise;
  expect(held(s.controller.getSnapshot())).toBeNull();
  expect(s.controller.getSnapshot().lastOutcome).toBe("Pasted into Example Writer");
});

test("an old delivery response cannot consume a reopened session's new citation", async () => {
  const old = deferred<DeliveryOutcome>(), fresh = deferred<DeliveryOutcome>();
  const s = setup({ deliverCitation: vi.fn().mockReturnValueOnce(old.promise).mockReturnValueOnce(fresh.promise) });
  s.enter();
  s.controller.dispatch({ type: "hidden", now: 10 });
  s.controller.dispatch({ type: "shown", now: 20, presentation: 2 });
  s.ready(); s.enter();
  old.resolve({ outcome: "copied-only", copied, reason: "no-permission" });
  await old.promise;
  expect(held(s.controller.getSnapshot())).toBe("apa");
  expect(s.controller.getSnapshot().permissionDetour).toBe(false);
  expect(s.controller.getSnapshot().activeCopy).toBe(2);
  fresh.resolve({ outcome: "pasted", copied, app: "Example Writer" });
  await fresh.promise;
  expect(held(s.controller.getSnapshot())).toBeNull();
});

test("a refused delivery renders its sentence and refreshes a changed snapshot", async () => {
  const s = setup({ deliverCitation: vi.fn(async (): Promise<DeliveryOutcome> => ({ outcome: "refused", problem: {
    code: "snapshot_changed", message: { id: "picker-snapshot-changed", args: {} },
    recovery: { action: "select-references" }, focus: "results",
  } })) });
  s.enter(); await Promise.resolve();
  expect(s.controller.getSnapshot().copyError).toBe("The library changed. Search again and reselect the references before copying.");
  expect(s.controller.getSnapshot().copied).toBeNull();
  expect(s.ports.refresh).toHaveBeenCalledOnce();
});

test("cancelled delivery quietly releases the flight and preserves the held format", async () => {
  const s = setup({ deliverCitation: vi.fn(async (): Promise<DeliveryOutcome> => ({ outcome: "cancelled" })) });
  s.enter(); await Promise.resolve();
  expect(s.controller.getSnapshot().activeCopy).toBeNull();
  expect(s.controller.getSnapshot().copied).toBeNull();
  expect(s.controller.getSnapshot().copyError).toBeNull();
  expect(held(s.controller.getSnapshot())).toBe("apa");
});

test("failed hiding reports the existing fixed error while confirming the completed copy", async () => {
  const s = setup({ deliverCitation: vi.fn(async (): Promise<DeliveryOutcome> => ({ outcome: "retained", copied, reason: "hide-failed" })) });
  s.enter(); await Promise.resolve();
  expect(s.ports.report).toHaveBeenCalledExactlyOnceWith("hide", true);
  expect(s.controller.getSnapshot().copied).toEqual(copied);
});

test("a transport failure shows a fixed sentence without exposing its payload", async () => {
  const s = setup({ deliverCitation: vi.fn().mockRejectedValue(new Error("invented private payload")) });
  s.enter(); await Promise.resolve();
  expect(s.controller.getSnapshot().copyError).toBe("Nothing was copied. Try again, or use Reload.");
  expect(s.ports.refresh).not.toHaveBeenCalled();
});
