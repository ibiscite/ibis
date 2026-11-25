import { expect, test, vi } from "vitest";
import type { DeliveryOutcome } from "../../bindings";
import { testStates } from "../../test-fixtures";
import { choiceKey } from "./choice";
import { arrangeView } from "../../testing/sessionView";
import { initialSession } from "./session";
import { queryKeyOf } from "./search";
import { createSessionController, type SessionPorts } from "./sessionController";

function setup(overrides: Partial<SessionPorts> = {}) {
  const chosen = [testStates[1]!.results[0]!];
  const [arranged] = arrangeView({ ...initialSession(), presentation: 0 }, { queryKey: queryKeyOf(initialSession()), format: "apa", snapshotRevision: "synthetic",
    defaultChoice: choiceKey(chosen), offered: chosen, chosen, shown: { display: { nodes: [] }, text: "canonical citation", warnings: [], fileTicket: "prepared", fileHash: "hash" },
    searchPending: false, ready: true, saving: false, shouldAsk: false, remembersLast: true, output: { opening: 0, choice: 0, delivery: 0 } });
  // A format picked for this citation only: the copy that uses it consumes it.
  const controller = createSessionController({ ...arranged, destination: { ...arranged.destination, held: "apa" } });
  const ports: SessionPorts = {
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

test("a transport failure shows a fixed sentence without exposing its payload", async () => {
  const s = setup({ deliverCitation: vi.fn().mockRejectedValue(new Error("invented private payload")) });
  s.enter(); await Promise.resolve();
  expect(s.controller.getSnapshot().copyError).toBe("Nothing was copied. Try again, or use Reload.");
  expect(s.ports.refresh).not.toHaveBeenCalled();
});
