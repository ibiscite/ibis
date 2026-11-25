import { expect, test } from "vitest";
import { testStates } from "../../test-fixtures";
import { choiceKey, idKey, type CitationReference } from "./choice";
import { arrangeView } from "../../testing/sessionView";
import { initialSession, reduceSession, type SessionEvent, type SessionView } from "./session";
import { queryKeyOf } from "./search";
import type { Copied, DeliveryOutcome } from "../../bindings";

const a: CitationReference = { ...testStates[1]!.results[0]!, locator: { term: "page", value: "١٢–١٤" } };
const prepared = { display: { nodes: [] }, text: "canonical engine text", warnings: [], fileTicket: "ticket-b", fileHash: "hash-b" };
function scenario() {
  let state = { ...initialSession(), presentation: 0 as number | null };
  let effects: ReturnType<typeof reduceSession>[1] = [];
  const send = (event: SessionEvent) => { [state, effects] = reduceSession(state, event); return effects; };
  const view = (change: Partial<SessionView> = {}): SessionEvent => {
    const [arranged, event] = arrangeView(state, { queryKey: queryKeyOf(state), format: "pandoc", snapshotRevision: "synthetic", defaultChoice: choiceKey([a]),
      offered: [a], chosen: [a], shown: prepared, searchPending: false, ready: true, saving: false, shouldAsk: false, remembersLast: true,
      output: { opening: 0, choice: 0, delivery: 0 }, ...change });
    // The screen sends what it shows, then commits it.
    [state] = reduceSession(arranged, event);
    return { type: "committed" };
  };
  const complete = (id: number, { output: _, pinned = false, outcome = { text: prepared.text, warnings: [] }, ...change }: { output?: unknown; pinned?: boolean; outcome?: Copied; session?: number; result?: DeliveryOutcome } = {}) => {
    const { result, ...owner } = change;
    const copied = { ...outcome, warnings: [...outcome.warnings, ...state.flights.find((f) => f.id === id)?.warnings ?? []] };
    return send({ type: "completed", id, session: 0, outcome: result ?? (pinned ? { outcome: "retained", copied, reason: "pinned" } : { outcome: "pasted", copied, app: "Example Writer" }), ...owner });
  };
  return { send, view, complete, get state() { return state; }, get effects() { return effects; } };
}

const copyOnce = (s: ReturnType<typeof scenario>, id: number, text: string) => {
  s.send(s.view({ appId: "example.writer", chosen: [...s.state.selected.values()], shown: { ...prepared, text } }));
  const effects = s.send({ type: "enter", choice: choiceKey([...s.state.selected.values()]) });
  s.send({ type: "completed", id, session: s.state.session, outcome: { outcome: "retained", copied: { text, warnings: [] }, reason: "pinned" } });
  return effects;
};
const withLast = () => {
  const s = scenario();
  s.send({ type: "toggle", result: a, locator: a.locator });
  s.send({ type: "reference", reference: s.state.selected.get(idKey(a.id))!, change: { form: "year-only" } });
  copyOnce(s, 1, "(2020, p. 12)");
  return s;
};

test("with Recent Citations off nothing is remembered, and turning it off forgets at once", () => {
  const off = withLast();
  off.send(off.view({ appId: "example.writer", remembersLast: false }));
  expect(off.state.lastCitations.size).toBe(0);
  off.send({ type: "change-last" });
  expect(off.state.changing).toBeNull();

  const s = scenario();
  s.send({ type: "toggle", result: a });
  s.send(s.view({ appId: "example.writer", remembersLast: false, chosen: [...s.state.selected.values()] }));
  s.send({ type: "enter", choice: choiceKey([...s.state.selected.values()]) });
  s.send({ type: "completed", id: 1, session: 0, outcome: { outcome: "retained", copied: { text: prepared.text, warnings: [] }, reason: "pinned" } });
  expect(s.state.lastCitations.size).toBe(0);
});
