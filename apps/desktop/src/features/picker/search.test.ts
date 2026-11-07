import { expect, test } from "vitest";
import { testStates } from "../../test-fixtures";
import { initialSession, reduceSession, type SessionEffect, type SessionEvent, type SessionState } from "./session";
import type { SearchInputs, SearchReply } from "./search";

const library = { ...testStates[0]!.library, revision: "1" };
const row = (key: string) => ({ ...testStates[1]!.results[0]!, id: { ...testStates[1]!.results[0]!.id, key }, revision: "1" });
const inputs = (query: string, overrides: Partial<SearchInputs> = {}): SearchInputs =>
  ({ queryKey: JSON.stringify([query, true]), query, interpretLocators: true, library, version: 0, recent: undefined, ...overrides });

/** Run events through the session, keeping every effect it asks for. */
function run(events: SessionEvent[], start: SessionState = initialSession()) {
  let state = start;
  const effects: SessionEffect[] = [];
  for (const event of events) {
    const [next, asked] = reduceSession(state, event);
    state = next;
    effects.push(...asked);
  }
  return { state, effects };
}
const searches = (effects: SessionEffect[]) => effects.filter((effect) => effect.type === "search");
const reply = (...keys: string[]): SearchReply => ({ revision: "1", results: keys.map(row) });

test("a search's reply shows its results, then its count is announced", () => {
  const { state, effects } = run([{ type: "search-inputs", inputs: inputs("garcia") }]);
  expect(searches(effects)).toEqual([{ type: "search", seq: 1, query: "garcia", interpretLocators: true, recent: false }]);
  const after = run([{ type: "searched", seq: 1, reply: reply("a", "b") }], state);
  expect(after.state.search.results.map((r) => r.id.key)).toEqual(["a", "b"]);
  expect(after.effects).toEqual([{ type: "announce-after", seq: 1, ms: 300 }]);
  expect(run([{ type: "announce", seq: 1 }], after.state).state.search.announcement).toBe("2 results");
});

test("a late reply to an older query never replaces the newer one's results", () => {
  const { state } = run([
    { type: "search-inputs", inputs: inputs("gar") },
    { type: "search-inputs", inputs: inputs("garcia") },
    { type: "searched", seq: 2, reply: reply("new") },
    { type: "searched", seq: 1, reply: reply("old") },
    { type: "announce", seq: 1 },
  ]);
  expect(state.search.results.map((r) => r.id.key)).toEqual(["new"]);
  expect(state.search.announcement).toBe("");
});

test("a failed search shows its error alone, and the query asked again searches again", () => {
  const failed = run([
    { type: "search-inputs", inputs: inputs("garcia") },
    { type: "searched", seq: 1, reply: reply("a") },
    { type: "search-inputs", inputs: inputs("okafor") },
    { type: "search-failed", seq: 2 },
  ]).state;
  expect(failed.search.results).toEqual([]);
  expect(failed.search.error).toBeTruthy();
  const again = run([{ type: "search-inputs", inputs: inputs("garcia") }], failed);
  expect(searches(again.effects)).toEqual([{ type: "search", seq: 3, query: "garcia", interpretLocators: true, recent: false }]);
  // The error stays until that search answers.
  expect(again.state.search.error).toBeTruthy();
});

test("typing back to an answered query adopts its answer and retires the newer search still out", () => {
  const { state } = run([
    { type: "search-inputs", inputs: inputs("a") },
    { type: "searched", seq: 1, reply: reply("first") },
    { type: "search-inputs", inputs: inputs("ab") },
    { type: "search-inputs", inputs: inputs("a") },
    // The "ab" reply arrives after the person went back to "a".
    { type: "searched", seq: 2, reply: reply("second") },
  ]);
  expect(state.search.results.map((r) => r.id.key)).toEqual(["first"]);
  expect(state.search.settled?.inputs.query).toBe("a");
});

test("reopening adopts a finished search and asks again for one still out", () => {
  const finished = run([
    { type: "search-inputs", inputs: inputs("garcia") },
    { type: "searched", seq: 1, reply: reply("a") },
    { type: "shown", now: 1, presentation: 1 },
    { type: "search-inputs", inputs: inputs("garcia") },
  ]);
  expect(searches(finished.effects)).toHaveLength(1);
  expect(finished.state.search.settled?.session).toBe(finished.state.session);
  const unfinished = run([
    { type: "search-inputs", inputs: inputs("garcia") },
    { type: "shown", now: 1, presentation: 1 },
    { type: "search-inputs", inputs: inputs("garcia") },
    { type: "searched", seq: 1, reply: reply("old") },
    { type: "searched", seq: 2, reply: reply("new") },
  ]);
  expect(searches(unfinished.effects)).toHaveLength(2);
  expect(unfinished.state.search.results.map((r) => r.id.key)).toEqual(["new"]);
});

test("before a library opens nothing is searched, the query is kept and the first row is highlighted", () => {
  const { state, effects } = run([
    { type: "highlight", index: 2 },
    { type: "search-inputs", inputs: inputs("garcia", { library: { ...library, state: "loading" } }) },
  ]);
  expect(searches(effects)).toHaveLength(0);
  expect(state.active).toBe(0);
  expect(state.search.resultsQuery).toBe(inputs("garcia").queryKey);
});

test("a reply from a newer snapshot reads the library status again", () => {
  const { effects } = run([
    { type: "search-inputs", inputs: inputs("garcia") },
    { type: "searched", seq: 1, reply: { revision: "2", results: [] } },
  ]);
  expect(effects).toContainEqual({ type: "refresh" });
});
