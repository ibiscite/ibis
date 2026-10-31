import { expect, test } from "vitest";
import { testStates } from "../../test-fixtures";
import { enterLabel } from "./enterLabel";
import { choiceKey, idKey, type CitationReference } from "./choice";
import { initialSession, reduceSession, sessionQueryKey, type SessionEvent, type SessionView } from "./session";

const a: CitationReference = { ...testStates[1]!.results[0]!, locator: { label: "page", value: "١٢–١٤" } };
const b = testStates[1]!.results[1]!;
const prepared = { text: "canonical engine text", html: "", markup: ["canonical engine text"], warnings: [], fileTicket: "ticket-b", fileHash: "hash-b" };
function scenario() {
  let state = initialSession();
  let effects: ReturnType<typeof reduceSession>[1] = [];
  const send = (event: SessionEvent) => { [state, effects] = reduceSession(state, event); return effects; };
  const view = (change: Partial<SessionView> = {}): SessionEvent => ({ type: "view", session: state.session, interaction: state.interaction,
    view: { queryKey: sessionQueryKey(state), format: "pandoc", snapshotRevision: "synthetic", defaultChoice: choiceKey([a]),
      chosen: [a], shown: prepared, previewVisible: true, searchPending: false, ready: true, saving: false, shouldAsk: false,
      output: { opening: 0, choice: 0 }, ...change } });
  const complete = (id: number, change: Partial<Extract<SessionEvent, { type: "copied" }>> = {}) => send({ type: "copied", id, session: 0, outcome: { text: prepared.text, warnings: [] },
    output: { opening: 0, choice: 0 }, pinned: false, ...change });
  return { send, view, complete, get state() { return state; }, get effects() { return effects; } };
}

test("removing a reference offers an owned Undo that restores its position and complete details", () => {
  const s = scenario();
  const c = { ...b, id: { synthetic: "third" }, key: "third" };
  s.send({ type: "toggle", result: a, locator: a.locator });
  s.send({ type: "reference", reference: s.state.selected.get(idKey(a.id))!, change: { form: "year-only" } });
  s.send({ type: "toggle", result: b });
  s.send({ type: "toggle", result: c });
  const original = [...s.state.selected.values()];
  s.send({ type: "toggle", result: b });
  const undo = s.state.removed!;
  expect([...s.state.selected.values()]).toEqual([original[0], original[2]]);
  s.send({ type: "undo-reference", removal: undo });
  expect([...s.state.selected.values()]).toEqual(original);
  expect(s.state.removed).toBeNull();
  s.send({ type: "toggle", result: a });
  const newer = s.state.removed;
  s.send({ type: "expire-removal", removal: undo });
  expect(s.state.removed).toBe(newer);
  s.send({ type: "undo-reference", removal: undo });
  expect(s.state.removed).toBe(newer);
  expect(s.state.selected.has(idKey(a.id))).toBe(false);
});

test("Undo never overwrites a reference added again or crosses a cleared session", () => {
  const s = scenario();
  s.send({ type: "toggle", result: a, locator: a.locator });
  s.send({ type: "toggle", result: a });
  const undo = s.state.removed!;
  s.send({ type: "toggle", result: a, locator: { label: "page", value: "42" } });
  s.send({ type: "undo-reference", removal: undo });
  expect(s.state.selected.get(idKey(a.id))?.locator?.value).toBe("42");
  s.send({ type: "toggle", result: a });
  const old = s.state.removed!;
  s.send({ type: "sample" });
  s.send({ type: "undo-reference", removal: old });
  expect(s.state.selected.size).toBe(0);
});

test("a retired segment cannot remove a newly edited reference or add a removed one back", () => {
  const s = scenario();
  s.send({ type: "toggle", result: a });
  const retired = s.state.selected.get(idKey(a.id))!;
  s.send({ type: "reference", reference: retired, change: { locator: { label: "page", value: "42" } } });
  s.send({ type: "remove-reference", reference: retired });
  const current = s.state.selected.get(idKey(a.id))!;
  expect(current.locator?.value).toBe("42");
  s.send({ type: "remove-reference", reference: current });
  s.send({ type: "remove-reference", reference: current });
  expect(s.state.selected.size).toBe(0);
  expect(s.state.removed?.reference).toBe(current);
});

test("the shelf confirms the copied text until a newer edit, and late completion cannot revive it", () => {
  const s = scenario();
  s.send(s.view());
  s.send({ type: "enter", choice: choiceKey([a]) });
  s.complete(1, { pinned: true });
  expect(s.state.shelfCopied?.text).toBe(prepared.text);
  s.send({ type: "query", value: "newer" });
  expect(s.state.shelfCopied).toBeNull();
  s.complete(1, { pinned: true });
  expect(s.state.shelfCopied).toBeNull();
});

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

test("a paste refusal retains its reason and a permission refusal retains the prepared shelf", () => {
  const s = scenario();
  s.send({ type: "toggle", result: a, locator: a.locator });
  s.send(s.view());
  s.send({ type: "enter", choice: choiceKey([a]) });
  s.complete(1);
  s.send({ type: "hid", id: 1, session: 0 });
  s.send({ type: "delivered", id: 1, session: 0, delivered: { outcome: "copied", reason: "no-permission" } });
  expect(s.state.lastOutcome).toBe("Copied. Ibis needs your OK to paste");
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
  s.send({ type: "copy-failed", id: 1, session: 0, output: { opening: 0, choice: 0 }, message: "Invented copy failure", stale: false });
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
  s.send({ type: "shown", now: 20 });
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
  s.send({ type: "shown", now: 20 });
  const output = { opening: 1, choice: 0 };
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
