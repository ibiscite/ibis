import { expect, test } from "vitest";
import { testStates } from "../../test-fixtures";
import { arrangeView } from "../../testing/sessionView";
import { choiceKey } from "./choice";
import { initialSession, reduceSession, sessionQueryKey, type SessionEvent, type SessionState, type SessionView } from "./session";

const a = testStates[1]!.results[0]!;
const prepared = { text: "canonical engine text", html: "", markup: [], warnings: [], fileTicket: "ticket-a", fileHash: "hash-a" };
const base = (state: SessionState): SessionView => ({ queryKey: sessionQueryKey(state), format: "pandoc", snapshotRevision: "synthetic", defaultChoice: choiceKey([a]),
  chosen: [a], shown: prepared, searchPending: false, ready: true, saving: false, shouldAsk: false, remembersLast: true, output: { opening: 0, choice: 0, delivery: 0 } });

/** A picker showing `a` with its preview, then `events`; returns what Enter asked for. */
function run(events: SessionEvent[], query = "garcia") {
  const [arranged, heard] = arrangeView({ ...initialSession(query), presentation: 0 }, base({ ...initialSession(query) }));
  let [state, effects] = reduceSession(arranged, heard);
  const asked = [...effects];
  for (const event of events) {
    [state, effects] = reduceSession(state, event);
    asked.push(...effects);
  }
  return { state, copies: asked.filter((effect) => effect.type === "copy") };
}
const enter: SessionEvent = { type: "enter", choice: choiceKey([a]) };

test("Enter copies the shown citation's own preview once", () => {
  const { copies } = run([enter, enter]);
  expect(copies).toMatchObject([{ ticket: "ticket-a", hash: "hash-a" }]);
});

test("a command query never copies", () => {
  expect(run([enter], ">change").copies).toEqual([]);
});

test("Enter right after typing waits for the new query's search instead of copying the old row", () => {
  const { state, copies } = run([{ type: "query", value: "garcia 2020" }, enter]);
  expect(copies).toEqual([]);
  expect(state.view?.searchPending).toBe(true);
});

test("Enter while reopening waits for the new opening's search, then copies the preview that carried over", () => {
  const reopened = run([{ type: "hidden", now: 10 }, { type: "shown", now: 20, presentation: 2 }, enter]);
  expect(reopened.copies).toEqual([]);
  const { state } = reopened;
  // The picker tells the new opening what it searches: the settled answer is adopted without asking again.
  const [heard, searched] = reduceSession(state, { type: "search-inputs", inputs: state.search.inputs! });
  expect(searched).toEqual([]);
  const [next, effects] = reduceSession(heard, { type: "committed" });
  expect(effects).toMatchObject([{ type: "copy", session: next.session, ticket: "ticket-a" }]);
});

test("Esc steps back: clearing the search drops the waiting Enter", () => {
  const { state, copies } = run([{ type: "query", value: "garcia 2020" }, enter, { type: "escape-search" }]);
  expect(state.pendingCopy).toBeNull();
  expect(state.query).toBe("");
  expect(copies).toEqual([]);
});

test.each([false, true])("a destination read that answers before the screen catches up waits for its commit (new default format: %s)", (changed) => {
  const [arranged, heard] = arrangeView({ ...initialSession("garcia"), presentation: 0 }, { ...base({ ...initialSession("garcia") }), appId: "example.writer" });
  let [state] = reduceSession(arranged, heard);
  [state] = reduceSession(state, { type: "read-destination", reset: false });
  [state] = reduceSession(state, enter);
  const [read, readEffects] = reduceSession(state, { type: "destination-read", request: state.destination.reads, destination: state.destination.destination! });
  expect(readEffects.filter((effect) => effect.type === "copy")).toEqual([]);
  // A new default format may still be on its way from the screen; the commit brings it first.
  const [newer] = changed ? reduceSession(read, { type: "picker-inputs", inputs: { ...read.inputs!, fallback: "typst" } }) : [read];
  const [, committed] = reduceSession(newer, { type: "committed" });
  // Typst has no preview yet, so the Pandoc one is never copied.
  expect(committed.filter((effect) => effect.type === "copy")).toHaveLength(changed ? 0 : 1);
});
