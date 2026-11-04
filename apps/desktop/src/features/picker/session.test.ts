import { expect, test } from "vitest";
import { testStates } from "../../test-fixtures";
import { enterLabel } from "./enterLabel";
import { choiceKey, idKey, type CitationReference } from "./choice";
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
      chosen: [a], shown: prepared, searchPending: false, ready: true, saving: false, shouldAsk: false, remembersLast: true,
      output: { opening: 0, choice: 0, delivery: 0 }, ...change } });
  const complete = (id: number, change: Partial<Extract<SessionEvent, { type: "copied" }>> = {}) => send({ type: "copied", id, session: 0, outcome: { text: prepared.text, warnings: [] },
    output: { opening: 0, choice: 0, delivery: 0 }, pinned: false, ...change });
  return { send, view, complete, get state() { return state; }, get effects() { return effects; } };
}

test.each(["paused", "conflict"] as const)("normal Enter refuses a %s project check without losing the preview or references", (project_check) => {
  const s = scenario();
  s.send({ type: "toggle", result: a, locator: a.locator });
  const shown = { ...prepared, project_check, project_message: "Check the invented bibliography." };
  s.send(s.view({ shown }));
  expect(s.send({ type: "enter", choice: choiceKey([a]) })).toEqual([]);
  expect(s.state.copyError).toBe(shown.project_message);
  expect(s.state.pendingCopy).toBeNull();
  expect(s.state.view?.shown).toBe(shown);
  expect([...s.state.selected.values()]).toMatchObject([{ key: a.key, locator: a.locator }]);
});

test("an explicit unchecked action permits just the paused citation and is never remembered", () => {
  const s = scenario();
  s.send(s.view({ shown: { ...prepared, project_check: "paused", project_message: "Could not check." } }));
  expect(s.send({ type: "enter", choice: choiceKey([a]), unverified: true })).toMatchObject([{ type: "copy", unverified: true }]);
  s.complete(1, { pinned: true, outcome: { text: prepared.text, warnings: ["Unverified project key."] } });
  expect(s.state.copied?.warnings).toEqual(["Unverified project key."]);
  s.send(s.view({ shown: { ...prepared, project_check: "paused", project_message: "Could not check." } }));
  expect(s.send({ type: "enter", choice: choiceKey([a]) })).toEqual([]);
  expect(s.state.copyError).toBe("Could not check.");
});

test("a known project conflict cannot be bypassed by an unchecked action", () => {
  const s = scenario();
  s.send(s.view({ shown: { ...prepared, project_check: "conflict", project_message: "This key names another work." } }));
  expect(s.send({ type: "enter", choice: choiceKey([a]), unverified: true })).toEqual([]);
  expect(s.state.copyError).toBe("This key names another work.");
});

test("a project check retry never releases an earlier normal insertion", () => {
  const s = scenario();
  s.send(s.view({ shown: { ...prepared, project_check: "paused" } }));
  s.send({ type: "enter", choice: choiceKey([a]) });
  expect(s.send(s.view({ shown: prepared }))).toEqual([]);
  expect(s.state.pendingCopy).toBeNull();
  expect(s.send({ type: "enter", choice: choiceKey([a]) })).toMatchObject([{ type: "copy" }]);
});

test("retained evidence permits normal delivery, while an independent closest-match warning still keeps the picker open", () => {
  const clean = scenario();
  clean.send(clean.view());
  clean.send({ type: "enter", choice: choiceKey([a]) });
  expect(clean.complete(1, { outcome: { text: prepared.text, warnings: ["Using the last checked bibliography."], project_warning_only: true } }))
    .toMatchObject([{ type: "hide" }]);
  const uncertain = scenario();
  uncertain.send(uncertain.view({ chosen: [{ ...a, partial: true }] }));
  uncertain.send({ type: "enter", choice: choiceKey([a]) });
  expect(uncertain.complete(1, { outcome: { text: prepared.text, warnings: ["Using the last checked bibliography."], project_warning_only: true } })
    .some((effect) => effect.type === "hide")).toBe(false);
  expect(uncertain.state.copied?.project_warning_only).toBe(false);
  expect(uncertain.state.copied?.warnings.length).toBe(2);
});

test("a project switch keeps search, ordered references and pages but cancels pending Enter and replacement", () => {
  const s = scenario();
  s.send({ type: "query", value: "synthetic" });
  s.send({ type: "toggle", result: a, locator: a.locator });
  s.send(s.view({ writingProjectId: "a", shown: null, searchPending: true }));
  s.send({ type: "enter", choice: null });
  expect(s.state.pendingCopy).not.toBeNull();
  s.send(s.view({ writingProjectId: "b", output: { opening: 0, choice: 1, delivery: 0 } }));
  expect(s.state.pendingCopy).toBeNull();
  expect(s.state.changing).toBeNull();
  expect(s.state.query).toBe("synthetic");
  expect([...s.state.selected.values()]).toMatchObject([{ key: a.key, locator: a.locator }]);
  expect(s.send(s.view({ writingProjectId: "b", output: { opening: 0, choice: 1, delivery: 0 } }))).toEqual([]);
});

test("a project switch does not offer another project's citation for replacement", () => {
  const s = scenario();
  s.send(s.view({ appId: "example.editor", writingProjectId: "a" }));
  s.send({ type: "enter", choice: choiceKey([a]) });
  s.complete(1, { pinned: true });
  expect(s.state.lastCitations.has("example.editor")).toBe(true);
  s.send({ type: "change-last" });
  expect(s.state.changing).toBe(prepared.text);
  s.send(s.view({ appId: "example.editor", writingProjectId: "b", output: { opening: 0, choice: 1, delivery: 0 } }));
  expect(s.state.changing).toBeNull();
  s.send({ type: "change-last" });
  expect(s.state.changing).toBeNull();
});

test.each([
  { writingProjectId: "project-b", format: "pandoc" as const },
  { writingProjectId: "project-a", format: "typst" as const },
  { writingProjectId: null, format: "pandoc" as const },
])("a last citation cannot cross project or output identity even before its change event: $writingProjectId $format", (next) => {
  const s = scenario();
  s.send(s.view({ appId: "example.editor", writingProjectId: "project-a" }));
  s.send({ type: "enter", choice: choiceKey([a]) });
  s.complete(1, { pinned: true });
  expect(s.state.lastCitations.get("example.editor")?.project).toBe("project-a");
  s.send(s.view({ appId: "example.editor", ...next }));
  s.send({ type: "change-last" });
  expect(s.state.changing).toBeNull();
});

test.each(["before", "after"] as const)("a refreshed checked receipt cannot adopt an earlier copy or let its response hide the picker (%s new Enter)", (oldResponse) => {
  const s = scenario();
  const first = { writingProjectId: "a", projectVersion: "1:0" };
  s.send(s.view(first));
  s.send({ type: "enter", choice: choiceKey([a]) });
  const checked = { writingProjectId: "a", projectVersion: "1:1", shown: { ...prepared, fileTicket: "checked-ticket", warnings: ["Using the last good bibliography copy."] } };
  s.send(s.view(checked));
  if (oldResponse === "before") {
    s.complete(1);
    expect(s.effects.some((effect) => effect.type === "hide" || effect.type === "notice")).toBe(false);
  }
  const second = s.send({ type: "enter", choice: choiceKey([a]) });
  expect(second).toMatchObject([{ type: "copy", id: 2, ticket: "checked-ticket" }]);
  if (oldResponse === "after") {
    s.complete(1);
    expect(s.effects.some((effect) => effect.type === "hide" || effect.type === "notice")).toBe(false);
  }
  expect(s.state.activeCopy).toBe(2);
  s.complete(2, { pinned: true, outcome: { text: prepared.text, warnings: checked.shown.warnings } });
  expect(s.state.copied?.warnings).toEqual(checked.shown.warnings);
});

test("a stale checked copy cannot consume the one-time output a fresh Enter is waiting for", () => {
  const s = scenario();
  const original = { format: "cite" as const, projectVersion: "1:0" };
  s.send(s.view(original));
  s.send({ type: "enter", choice: choiceKey([a]) });
  const refreshing = { format: "cite" as const, projectVersion: "1:1", shown: null };
  s.send(s.view(refreshing));
  s.send({ type: "enter", choice: choiceKey([a]) });
  expect(s.state.pendingCopy).not.toBeNull();
  s.complete(1, { pinned: true });
  expect(s.effects).toEqual([]);
  expect(s.state.pendingCopy).not.toBeNull();
  expect(s.send(s.view({ ...refreshing, shown: { ...prepared, fileTicket: "fresh-checked" } })))
    .toMatchObject([{ type: "copy", id: 2, ticket: "fresh-checked" }]);
});

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
  ["no-permission", "Copied. Ibis needs your OK to paste. Press Ctrl+V to paste."],
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

test.each(["same", "different", "empty", "failed", "reopened"])("Enter waiting for the destination survives only the same refreshed recent reference (%s)", (outcome) => {
  const s = scenario();
  s.send(s.view({ ready: false }));
  s.send({ type: "enter", choice: choiceKey([a]) });
  // Resolving the captured app refreshes empty-query recents. The old rows
  // temporarily stop being actionable while this request is pending.
  s.send(s.view({ appId: "example.browser", chosen: [], shown: null, searchPending: true, shouldAsk: true }));
  expect(s.effects).toEqual([]);
  if (outcome === "reopened") {
    s.send({ type: "hidden", now: 10 });
    s.send({ type: "shown", now: 20, presentation: 2 });
  }
  const chosen = outcome === "different" ? [b] : outcome === "empty" || outcome === "failed" ? [] : [a];
  s.send(s.view({ appId: "example.browser", chosen, shouldAsk: true, shown: outcome === "failed" ? "failed" : prepared }));
  if (outcome === "same") {
    expect(s.effects).toEqual([{ type: "ask-format" }]);
    s.send({ type: "question", answer: "answered" });
    s.send(s.view({ appId: "example.browser" }));
    expect(s.effects).toMatchObject([{ type: "copy", id: 1, ticket: "ticket-b", hash: "hash-b" }]);
  } else {
    expect(s.effects).toEqual([]);
    expect(s.state.pendingCopy).toBeNull();
  }
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
  expect(s.state.shelfCopied).toEqual({ text: prepared.text, warnings: [], closestMatches: [] });
  expect(s.state.copyNotice).toBeNull();
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

test("preparing clears an expired search before the window appears, and showing does not repeat it", () => {
  const s = scenario();
  s.send({ type: "query", value: "old" });
  s.send({ type: "hidden", now: 0 });
  s.send({ type: "preparing", now: 61_000 });
  expect(s.state.query).toBe("");
  expect(s.state.hiddenAt).toBeNull();
  s.send({ type: "shown", now: 61_100, presentation: 2 });
  expect(s.state.presentation).toBe(2);
});

test("short selected text opens as a search that lasts one opening", () => {
  const s = scenario();
  s.send({ type: "hidden", now: 0 });
  s.send({ type: "preparing", now: 10, selection: "smith 2020" });
  s.send({ type: "shown", now: 11, presentation: 1 });
  expect(s.state).toMatchObject({ query: "smith 2020", fromDocument: true, selectAll: 1 });
  s.send({ type: "hidden", now: 20 });
  s.send({ type: "preparing", now: 30, selection: null });
  s.send({ type: "shown", now: 31, presentation: 2 });
  expect(s.state).toMatchObject({ query: "", fromDocument: false });
});

test("a newer selection replaces an earlier search and keeps the shelf", () => {
  const s = scenario();
  s.send({ type: "toggle", result: a });
  s.send({ type: "query", value: "typed earlier" });
  s.send({ type: "hidden", now: 0 });
  s.send({ type: "preparing", now: 10, selection: "smith 2020" });
  expect(s.state.query).toBe("smith 2020");
  expect([...s.state.selected.values()]).toHaveLength(1);
});

test("no selection leaves a recent typed search to return as before", () => {
  const s = scenario();
  s.send({ type: "query", value: "typed earlier" });
  s.send({ type: "hidden", now: 0 });
  s.send({ type: "preparing", now: 10, selection: null });
  s.send({ type: "shown", now: 11, presentation: 1 });
  expect(s.state).toMatchObject({ query: "typed earlier", fromDocument: false });
});

test("editing document text makes it the person's own search", () => {
  const s = scenario();
  s.send({ type: "preparing", now: 10, selection: "smith 2020" });
  s.send({ type: "query", value: "smith 20" });
  s.send({ type: "hidden", now: 20 });
  s.send({ type: "preparing", now: 30 });
  expect(s.state).toMatchObject({ query: "smith 20", fromDocument: false });
});

test("Escape clears document text for good: it never comes back as a search", () => {
  const s = scenario();
  s.send({ type: "preparing", now: 10, selection: "smith 2020" });
  s.send({ type: "escape-search" });
  expect(s.state).toMatchObject({ query: "", fromDocument: false });
  s.send({ type: "hidden", now: 20 });
  s.send({ type: "preparing", now: 30, selection: null });
  expect(s.state.query).toBe("");
});

test("a new selection ends a permission detour: the text a paste would replace is always shown", () => {
  const s = scenario();
  s.send({ type: "toggle", result: a });
  s.send({ type: "query", value: "prepared" });
  s.send(s.view());
  s.send({ type: "enter", choice: choiceKey([a]) });
  s.complete(1);
  s.send({ type: "hid", id: 1, session: 0 });
  s.send({ type: "delivered", id: 1, session: 0, delivered: { outcome: "copied", reason: "no-permission" } });
  expect(s.state.permissionDetour).toBe(true);
  s.send({ type: "hidden", now: 20 });
  s.send({ type: "preparing", now: 30, selection: "smith 2020" });
  expect(s.state).toMatchObject({ query: "smith 2020", fromDocument: true, permissionDetour: false });
  expect([...s.state.selected.values()]).toHaveLength(1);
});

test("a detour keeps its prepared search when the document has no selection", () => {
  const s = scenario();
  s.send({ type: "query", value: "prepared" });
  s.send(s.view());
  s.send({ type: "enter", choice: choiceKey([a]) });
  s.complete(1);
  s.send({ type: "hid", id: 1, session: 0 });
  s.send({ type: "delivered", id: 1, session: 0, delivered: { outcome: "copied", reason: "no-permission" } });
  s.send({ type: "hidden", now: 20 });
  s.send({ type: "preparing", now: 30, selection: null });
  expect(s.state).toMatchObject({ query: "prepared", permissionDetour: true });
});

test("a fresh Enter after a detour ends it, so a reopen during delivery cannot leave it stuck", () => {
  const s = scenario();
  s.send({ type: "query", value: "prepared" });
  s.send(s.view());
  s.send({ type: "enter", choice: choiceKey([a]) });
  s.complete(1);
  s.send({ type: "hid", id: 1, session: 0 });
  s.send({ type: "delivered", id: 1, session: 0, delivered: { outcome: "copied", reason: "no-permission" } });
  s.send({ type: "hidden", now: 20 });
  s.send({ type: "preparing", now: 30, selection: null });
  s.send({ type: "shown", now: 31, presentation: 0 });
  expect(s.state.permissionDetour).toBe(true);
  s.send(s.view());
  s.send({ type: "enter", choice: choiceKey([a]) });
  s.send({ type: "copied", id: 2, session: s.state.session, outcome: { text: prepared.text, warnings: [] }, output: { opening: 0, choice: 0, delivery: 0 }, pinned: false });
  expect(s.state.permissionDetour).toBe(false);
});

test("a detour keeps a citation prepared from document text", () => {
  const s = scenario();
  s.send({ type: "preparing", now: 1, selection: "smith 2020" });
  s.send(s.view());
  s.send({ type: "enter", choice: choiceKey([a]) });
  s.complete(1);
  s.send({ type: "hid", id: 1, session: 0 });
  s.send({ type: "delivered", id: 1, session: 0, delivered: { outcome: "copied", reason: "no-permission" } });
  s.send({ type: "hidden", now: 20 });
  s.send({ type: "preparing", now: 30, selection: null });
  expect(s.state.query).toBe("smith 2020");
});

test("reopening during delivery is no detour: the new selection opens, and the late refusal cannot stick", () => {
  const s = scenario();
  s.send({ type: "query", value: "prepared" });
  s.send(s.view());
  s.send({ type: "enter", choice: choiceKey([a]) });
  s.complete(1);
  s.send({ type: "hid", id: 1, session: 0 });
  s.send({ type: "hidden", now: 20 });
  s.send({ type: "preparing", now: 21, selection: "jones 2021" });
  s.send({ type: "shown", now: 22, presentation: 1 });
  expect(s.state).toMatchObject({ query: "jones 2021", fromDocument: true });
  s.send({ type: "delivered", id: 1, session: 0, delivered: { outcome: "copied", reason: "no-permission" } });
  expect(s.state.permissionDetour).toBe(false);
  s.send({ type: "hidden", now: 30 });
  s.send({ type: "preparing", now: 31, selection: "newer 2022" });
  expect(s.state.query).toBe("newer 2022");
});

test("a selection arriving with a waiting Enter cancels it: Enter never pastes a citation from an older search", () => {
  const s = scenario();
  s.send(s.view({ searchPending: true }));
  s.send({ type: "enter", choice: null });
  expect(s.state.pendingCopy).not.toBeNull();
  s.send({ type: "preparing", now: 10, selection: "smith 2020" });
  expect(s.state.pendingCopy).toBeNull();
});

const copyOnce = (s: ReturnType<typeof scenario>, id: number, text: string) => {
  s.send(s.view({ appId: "example.writer", chosen: [...s.state.selected.values()], shown: { ...prepared, text } }));
  const effects = s.send({ type: "enter", choice: choiceKey([...s.state.selected.values()]) });
  s.send({ type: "copied", id, session: s.state.session, outcome: { text, warnings: [] }, output: { opening: 0, choice: 0, delivery: 0 }, pinned: true });
  return effects;
};
const withLast = () => {
  const s = scenario();
  s.send({ type: "toggle", result: a, locator: a.locator });
  s.send({ type: "reference", reference: s.state.selected.get(idKey(a.id))!, change: { form: "year-only" } });
  copyOnce(s, 1, "(2020, p. 12)");
  return s;
};

test("Change Last reopens the app's last citation with its pages and options, and clears the search", () => {
  const s = withLast();
  expect(s.state.selected.size).toBe(0);
  s.send({ type: "query", value: "something" });
  s.send(s.view({ appId: "example.writer" }));
  s.send({ type: "change-last" });
  expect(s.state).toMatchObject({ query: "", changing: "(2020, p. 12)" });
  const [reference] = [...s.state.selected.values()];
  expect(reference).toMatchObject({ locator: a.locator, form: "year-only" });
});

test("only the changed citation's copy carries the text it may replace", () => {
  const s = withLast();
  s.send(s.view({ appId: "example.writer" }));
  s.send({ type: "change-last" });
  s.send({ type: "reference", reference: [...s.state.selected.values()][0]!, change: { form: "normal" } });
  const [copy] = copyOnce(s, 2, "(Smith, 2020, p. 12)");
  expect(copy).toMatchObject({ type: "copy", replaces: "(2020, p. 12)" });
  expect(s.state.changing).toBeNull();
  s.send({ type: "toggle", result: b });
  const [plain] = copyOnce(s, 3, "(Jones, 2021)");
  expect(plain).toMatchObject({ type: "copy", replaces: null });
});

test("the shortcut alone starts a new search: hiding abandons a change", () => {
  const s = withLast();
  s.send(s.view({ appId: "example.writer" }));
  s.send({ type: "change-last" });
  s.send({ type: "hidden", now: 10 });
  s.send({ type: "preparing", now: 20 });
  s.send({ type: "shown", now: 21, presentation: 1 });
  expect(s.state).toMatchObject({ changing: null, query: "" });
  expect(s.state.selected.size).toBe(0);
});

test("a permission detour keeps the change, so a fresh Enter still may only replace the old citation", () => {
  const s = withLast();
  s.send(s.view({ appId: "example.writer" }));
  s.send({ type: "change-last" });
  s.send(s.view({ appId: "example.writer", chosen: [...s.state.selected.values()] }));
  s.send({ type: "enter", choice: choiceKey([...s.state.selected.values()]) });
  expect(s.effects).toMatchObject([{ type: "copy", replaces: "(2020, p. 12)" }]);
  s.send({ type: "copied", id: 2, session: 0, outcome: { text: prepared.text, warnings: [] }, output: { opening: 0, choice: 0, delivery: 0 }, pinned: false });
  s.send({ type: "hidden", now: 10 });
  expect(s.state.changing).toBe("(2020, p. 12)");
  s.send({ type: "hid", id: 2, session: 0 });
  s.send({ type: "delivered", id: 2, session: 0, delivered: { outcome: "copied", reason: "no-permission" } });
  expect(s.state.changing).toBe("(2020, p. 12)");
  expect(s.state.selected.size).toBe(1);
  s.send({ type: "hidden", now: 20 });
  s.send({ type: "preparing", now: 30 });
  s.send({ type: "shown", now: 31, presentation: 0 });
  expect(s.state.changing).toBe("(2020, p. 12)");
  s.send(s.view({ appId: "example.writer", chosen: [...s.state.selected.values()] }));
  s.send({ type: "enter", choice: choiceKey([...s.state.selected.values()]) });
  expect(s.effects).toMatchObject([{ type: "copy", replaces: "(2020, p. 12)" }]);
});

test("a selection ending a detour leaves the change it held, so Enter still proves the old citation", () => {
  const s = withLast();
  s.send(s.view({ appId: "example.writer" }));
  s.send({ type: "change-last" });
  s.send(s.view({ appId: "example.writer", chosen: [...s.state.selected.values()] }));
  s.send({ type: "enter", choice: choiceKey([...s.state.selected.values()]) });
  s.send({ type: "copied", id: 2, session: 0, outcome: { text: prepared.text, warnings: [] }, output: { opening: 0, choice: 0, delivery: 0 }, pinned: false });
  s.send({ type: "hid", id: 2, session: 0 });
  s.send({ type: "delivered", id: 2, session: 0, delivered: { outcome: "copied", reason: "no-permission" } });
  s.send({ type: "hidden", now: 10 });
  s.send({ type: "preparing", now: 20, selection: "(2020, p. 12)" });
  expect(s.state).toMatchObject({ changing: "(2020, p. 12)", permissionDetour: false, query: "(2020, p. 12)" });
  expect(s.state.selected.size).toBe(1);
});

test("reopening while the changed citation is on its way abandons the change", () => {
  const s = withLast();
  s.send(s.view({ appId: "example.writer" }));
  s.send({ type: "change-last" });
  s.send(s.view({ appId: "example.writer", chosen: [...s.state.selected.values()] }));
  s.send({ type: "enter", choice: choiceKey([...s.state.selected.values()]) });
  s.send({ type: "hidden", now: 10 });
  expect(s.state.changing).toBe("(2020, p. 12)");
  s.send({ type: "preparing", now: 20 });
  s.send({ type: "shown", now: 21, presentation: 1 });
  expect(s.state.changing).toBeNull();
  expect(s.state.selected.size).toBe(0);
});

test("an older copy in flight cannot keep a new change past hiding", () => {
  const s = withLast();
  s.send({ type: "toggle", result: b });
  s.send(s.view({ appId: "example.writer", chosen: [...s.state.selected.values()] }));
  s.send({ type: "enter", choice: choiceKey([...s.state.selected.values()]) });
  s.send({ type: "change-last" });
  expect(s.state.activeCopy).not.toBeNull();
  s.send({ type: "hidden", now: 10 });
  expect(s.state.changing).toBeNull();
});

test("Enter on the same shelf joins a copy only when both may replace the same text", () => {
  const s = withLast();
  s.send({ type: "toggle", result: a, locator: a.locator });
  s.send({ type: "reference", reference: s.state.selected.get(idKey(a.id))!, change: { form: "year-only" } });
  const chosen = [...s.state.selected.values()];
  s.send(s.view({ appId: "example.writer", chosen }));
  s.send({ type: "enter", choice: choiceKey(chosen) });
  expect(s.state.flights).toHaveLength(1);
  s.send({ type: "change-last" });
  s.send(s.view({ appId: "example.writer", chosen: [...s.state.selected.values()] }));
  const [copy] = s.send({ type: "enter", choice: choiceKey([...s.state.selected.values()]) });
  expect(copy).toMatchObject({ type: "copy", replaces: "(2020, p. 12)" });
  expect(s.state.flights).toHaveLength(2);
});

test("a change remembers the format the citation was written in", () => {
  const s = withLast();
  expect(s.state.lastCitations.get("example.writer")?.format).toBe("pandoc");
});

test("each app has its own last citation, and an app without one changes nothing", () => {
  const s = withLast();
  s.send(s.view({ appId: "example.other" }));
  s.send({ type: "change-last" });
  expect(s.state.changing).toBeNull();
  expect(s.state.selected.size).toBe(0);
});

test("memory stays small: the app used longest ago is dropped", () => {
  const s = scenario();
  for (let id = 1; id <= 9; id += 1) {
    s.send({ type: "toggle", result: a });
    s.send(s.view({ appId: `example.app${id}`, chosen: [...s.state.selected.values()], shown: { ...prepared, text: `(${id})` }, defaultChoice: `${id}` }));
    s.send({ type: "enter", choice: choiceKey([...s.state.selected.values()]) });
    s.send({ type: "copied", id, session: 0, outcome: { text: `(${id})`, warnings: [] }, output: { opening: 0, choice: 0, delivery: 0 }, pinned: true });
  }
  expect(s.state.lastCitations.size).toBe(8);
  expect(s.state.lastCitations.has("example.app1")).toBe(false);
});

test("Forget clears the memory and an open change with it", () => {
  const s = withLast();
  s.send(s.view({ appId: "example.writer" }));
  s.send({ type: "change-last" });
  s.send({ type: "forget-last" });
  expect(s.state.lastCitations.size).toBe(0);
  expect(s.state.changing).toBeNull();
});

test("removing the last reference ends the change", () => {
  const s = withLast();
  s.send(s.view({ appId: "example.writer" }));
  s.send({ type: "change-last" });
  s.send({ type: "toggle", result: [...s.state.selected.values()][0]! });
  expect(s.state.changing).toBeNull();
});

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
  s.send({ type: "copied", id: 1, session: 0, outcome: { text: prepared.text, warnings: [] }, output: { opening: 0, choice: 0, delivery: 0 }, pinned: true });
  expect(s.state.lastCitations.size).toBe(0);
});

test("an app with no ID never shares a last citation with another", () => {
  const s = scenario();
  s.send({ type: "toggle", result: a });
  s.send(s.view({ appId: null, chosen: [...s.state.selected.values()] }));
  s.send({ type: "enter", choice: choiceKey([...s.state.selected.values()]) });
  s.send({ type: "copied", id: 1, session: 0, outcome: { text: prepared.text, warnings: [] }, output: { opening: 0, choice: 0, delivery: 0 }, pinned: true });
  expect(s.state.lastCitations.size).toBe(0);
});
