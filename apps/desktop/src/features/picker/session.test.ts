import { expect, test } from "vitest";
import { testStates } from "../../test-fixtures";
import { enterLabel } from "./enterLabel";
import { choiceKey, type CitationReference } from "./choice";
import { initialSession, reduceSession, sessionQueryKey, type SessionEvent, type SessionView } from "./session";

const a: CitationReference = { ...testStates[1]!.results[0]!, locator: { label: "page", value: "١٢–١٤" } };
const b = testStates[1]!.results[1]!;
const prepared = { text: "canonical engine text", html: "", markup: ["canonical engine text"], warnings: [], fileTicket: "ticket-b", fileHash: "hash-b" };
function scenario() {
  let state = { ...initialSession(), presentation: 0 as number | null };
  let effects: ReturnType<typeof reduceSession>[1] = [];
  const send = (event: SessionEvent) => { [state, effects] = reduceSession(state, event); return effects; };
  const view = (change: Partial<SessionView> = {}): SessionEvent => ({ type: "view", session: state.session, interaction: state.interaction,
    view: { queryKey: sessionQueryKey(state), format: "pandoc", snapshotRevision: "synthetic", defaultChoice: choiceKey([a]),
      chosen: [a], shown: prepared, searchPending: false, ready: true, saving: false, shouldAsk: false,
      output: { opening: 0, choice: 0, delivery: 0 }, ...change } });
  const complete = (id: number, change: Partial<Extract<SessionEvent, { type: "copied" }>> = {}) => send({ type: "copied", id, session: 0, outcome: { text: prepared.text, warnings: [] },
    output: { opening: 0, choice: 0, delivery: 0 }, pinned: false, ...change });
  return { send, view, complete, get state() { return state; }, get effects() { return effects; } };
}

test("search, Enter and receipt use exactly one prepared engine ticket", () => {
  const s = scenario();
  s.send(s.view({ chosen: [], shown: null, searchPending: true }));
  s.send({ type: "enter", choice: null });
  expect(s.effects).toEqual([]);
  s.send(s.view({ shown: null }));
  expect(s.effects).toEqual([]);
  expect(enterLabel(s.state.view!.format, s.state.view!.chosen.length, "Example Writer", true, false, false)).toBe("Paste into Example Writer · Pandoc");
  s.send(s.view());
  expect(s.effects).toMatchObject([{ type: "copy", id: 1, ticket: "ticket-b", hash: "hash-b" }]);
  s.send({ type: "enter", choice: choiceKey([a]) });
  expect(s.effects).toEqual([]);
  s.complete(1);
  expect(s.state.copied?.text).toBe(prepared.text);
  expect(s.effects).toMatchObject([{ type: "hide", id: 1 }]);
  s.send({ type: "hidden", now: 10 });
  s.send({ type: "hid", id: 1, session: 0 });
  expect(s.effects).toMatchObject([{ type: "hide-status", failed: false }, { type: "notice", outcome: { text: prepared.text } }]);
  s.send({ type: "delivered", id: 1, session: 0, delivered: { outcome: "pasted", app: "Example Writer" } });
  expect(s.state.lastOutcome).toBe("Pasted into Example Writer");
  expect(s.state.flights).toHaveLength(0);
});

test("exhausted presentation reads refuse Enter and later readiness cannot replay it", () => {
  const s = scenario();
  s.send({ type: "retire" });
  s.send(s.view());
  s.send({ type: "initialize" });
  s.send({ type: "enter", choice: choiceKey([a]) });
  const visibility = s.state.visibility;
  s.send({ type: "presentation", visibility, presentation: null });
  s.send({ type: "presentation", visibility, presentation: null });
  expect(s.state.copyError).toBe("Nothing was copied. Try again, or use Reload.");
  expect(s.state.pendingCopy).toBeNull();
  expect(s.effects).toEqual([]);
  s.send({ type: "initialize" });
  s.send({ type: "presentation", visibility, presentation: 41 });
  expect(s.effects).toEqual([]);
  s.send({ type: "enter", choice: choiceKey([a]) });
  expect(s.effects).toMatchObject([{ type: "copy", ticket: "ticket-b", hash: "hash-b" }]);
  expect(s.state.flights[0]?.presentation).toBe(41);
});

test.each([
  ["no-permission", "Copied. Ibis needs your OK to paste"],
  ["accessibility-required", "Copied. Ibis needs Accessibility access to paste safely. Press Ctrl+V to paste."],
])("a %s refusal retains its reason and prepared shelf", (reason, message) => {
  const s = scenario();
  s.send({ type: "toggle", result: a, locator: a.locator });
  s.send(s.view());
  s.send({ type: "enter", choice: choiceKey([a]) });
  s.complete(1);
  s.send({ type: "hid", id: 1, session: 0 });
  s.send({ type: "delivered", id: 1, session: 0, delivered: { outcome: "copied", reason } });
  expect(s.state.lastOutcome).toBe(message);
  expect([...s.state.selected.values()]).toEqual([a]);
  expect(s.state.permissionDetour).toBe(true);
});

test("search and preview failures never copy, and a copy rejection shows its fixed error", () => {
  const s = scenario();
  s.send(s.view({ chosen: [], shown: null, searchPending: true }));
  s.send({ type: "enter", choice: null });
  s.send(s.view({ chosen: [], shown: null }));
  expect(s.state.pendingCopy).toBeNull();
  expect(s.effects).toEqual([]);
  s.send(s.view({ shown: "failed" }));
  s.send({ type: "enter", choice: choiceKey([a]) });
  expect(s.effects).toEqual([]);
  s.send(s.view());
  s.send({ type: "enter", choice: choiceKey([a]) });
  s.send({ type: "copy-failed", id: 1, session: 0, output: { opening: 0, choice: 0, delivery: 0 }, message: "Invented copy failure", stale: false });
  expect(s.state.copyError).toBe("Invented copy failure");
  expect(s.effects).toEqual([]);
});

test("delayed A after B cannot replace B's Enter or prepared output", () => {
  const s = scenario();
  s.send({ type: "query", value: "A" });
  const old = s.view();
  s.send({ type: "query", value: "B" });
  s.send(s.view({ chosen: [b], defaultChoice: choiceKey([b]) }));
  s.send({ type: "enter", choice: choiceKey([b]) });
  expect(s.effects).toMatchObject([{ type: "copy", ticket: "ticket-b" }]);
  s.send(old);
  expect(s.state.view?.chosen).toEqual([b]);
  expect(s.effects).toEqual([]);
});

test("close/reopen retires old completion without consuming a newer copy", () => {
  const s = scenario();
  s.send(s.view());
  s.send({ type: "enter", choice: choiceKey([a]) });
  s.send({ type: "hidden", now: 10 });
  s.send({ type: "shown", now: 20, presentation: 2 });
  s.send(s.view());
  expect(s.effects).toEqual([]);
  s.send({ type: "enter", choice: choiceKey([a]) });
  expect(s.effects).toMatchObject([{ type: "copy", id: 2, session: 1 }]);
  s.complete(1);
  expect(s.effects).toEqual([]);
  expect(s.state.activeCopy).toBe(2);
  expect(s.state.copied).toBeNull();
  s.complete(2, { session: 1 });
  expect(s.effects).toMatchObject([{ type: "hide", id: 2 }]);
});

test("Shift+Enter keeps insertion order and canonical locators; Escape only cancels Enter", () => {
  const s = scenario();
  s.send({ type: "toggle", result: b });
  s.send({ type: "toggle", result: a, locator: a.locator });
  expect([...s.state.selected.values()]).toEqual([{ ...b, locator: null }, a]);
  s.send(s.view({ shown: null }));
  s.send({ type: "enter", choice: choiceKey([a]) });
  s.send({ type: "cancel" });
  s.send(s.view());
  expect(s.effects).toEqual([]);
  expect(s.state.selected.size).toBe(2);
});

test("Enter during reopening waits for the new opening's readiness and output", () => {
  const s = scenario();
  s.send(s.view());
  s.send({ type: "hidden", now: 10 });
  s.send({ type: "shown", now: 20, presentation: 2 });
  const output = { opening: 1, choice: 0, delivery: 0 };
  s.send({ type: "enter", choice: choiceKey([a]), output });
  expect(s.effects).toEqual([]);
  s.send(s.view({ ready: false, output }));
  expect(s.effects).toEqual([]);
  s.send(s.view({ output }));
  expect(s.effects).toMatchObject([{ type: "copy", session: 1 }]);
});

test("a current ready Enter joins a copy after query changes and still delivers once", () => {
  const s = scenario();
  s.send({ type: "toggle", result: a, locator: a.locator });
  s.send(s.view());
  s.send({ type: "enter", choice: choiceKey([a]) });
  s.send({ type: "query", value: "next reference" });
  s.send(s.view({ searchPending: true }));
  s.send({ type: "enter", choice: choiceKey([a]) });
  expect(s.effects).toEqual([]);
  expect(s.state.flights).toHaveLength(1);
  s.complete(1);
  expect(s.effects).toMatchObject([{ type: "hide" }]);
  s.complete(1);
  expect(s.effects).toEqual([]);
});

test("a stale success never consumes the newer citation's copy or receipt", () => {
  const s = scenario();
  s.send(s.view());
  s.send({ type: "enter", choice: choiceKey([a]) });
  s.send({ type: "highlight", index: 1 });
  s.send(s.view({ chosen: [b], defaultChoice: choiceKey([b]) }));
  s.send({ type: "enter", choice: choiceKey([b]) });
  s.complete(1);
  expect(s.effects).toEqual([]);
  expect(s.state.activeCopy).toBe(2);
  s.complete(2, { outcome: { text: "new citation", warnings: ["Invented warning"] } });
  expect(s.state.copied?.text).toBe("new citation");
  const receipt = s.state.copied;
  s.complete(1);
  expect(s.state.copied).toBe(receipt);
  expect(s.effects).toEqual([]);
});

test("delivery revocation keeps completed-write feedback without hiding or delivering", () => {
  const s = scenario();
  s.send({ type: "toggle", result: a, locator: a.locator });
  s.send(s.view());
  s.send({ type: "enter", choice: choiceKey([a]) });
  const output = { opening: 0, choice: 0, delivery: 1 };
  s.send(s.view({ output }));
  s.complete(1, { output, outcome: { text: prepared.text, warnings: [], delivery: 17 } });
  expect(s.state.copied).toEqual({ text: prepared.text, warnings: [] });
  expect(s.state.lastOutcome).toBe("Copied");
  expect(s.state.selected.size).toBe(0);
  expect(s.state.flights).toHaveLength(0);
  expect(s.effects).toEqual([{ type: "consume", output }]);
});

test.each(["before", "after"])("a revoked write arriving %s a newer copy's response cannot consume its feedback", (order) => {
  const s = scenario();
  s.send(s.view());
  s.send({ type: "enter", choice: choiceKey([a]) });
  const output = { opening: 0, choice: 0, delivery: 1 };
  s.send(s.view({ output }));
  s.send({ type: "enter", choice: choiceKey([a]) });
  expect(s.effects).toMatchObject([{ type: "copy", id: 2 }]);
  if (order === "before") {
    s.complete(1, { output });
    expect(s.state.activeCopy).toBe(2);
    expect(s.state.copied).toBeNull();
    expect(s.effects).toEqual([]);
  }
  s.complete(2, { output, pinned: true, outcome: { text: "new citation", warnings: [] } });
  expect(s.state.copied?.text).toBe("new citation");
  expect(s.effects).toEqual([{ type: "consume", output }]);
  if (order === "after") {
    const copied = s.state.copied;
    s.complete(1, { output });
    expect(s.state.copied).toBe(copied);
    expect(s.state.lastOutcome).toBe("Copied");
    expect(s.effects).toEqual([]);
  }
});

test("a revoked write cannot consume a fresh Enter waiting for its delivery save", () => {
  const s = scenario();
  s.send(s.view());
  s.send({ type: "enter", choice: choiceKey([a]) });
  const output = { opening: 0, choice: 0, delivery: 1 };
  s.send(s.view({ output, saving: true }));
  s.send({ type: "enter", choice: choiceKey([a]) });
  const pending = s.state.pendingCopy;
  s.complete(1, { output });
  expect(s.state.pendingCopy).toBe(pending);
  expect(s.state.copied).toBeNull();
  expect(s.effects).toEqual([]);
  s.send(s.view({ output }));
  expect(s.effects).toMatchObject([{ type: "copy", id: 2 }]);
});

test("an older confirmation timer cannot dismiss a newer copy's warnings", () => {
  const s = scenario();
  s.send(s.view());
  s.send({ type: "enter", choice: choiceKey([a]) });
  s.complete(1, { pinned: true });
  const copied = s.state.copied!;
  s.send(s.view());
  s.send({ type: "enter", choice: choiceKey([a]) });
  s.complete(2, { outcome: { text: "new citation", warnings: ["Invented warning"] } });
  s.send({ type: "dismiss", copied });
  expect(s.state.copied?.warnings).toEqual(["Invented warning"]);
});
