import { expect, test } from "vitest";
import type { FilePreview } from "../../bindings";
import { initialSession, reduceSession, type SessionEffect, type SessionEvent, type SessionState } from "./session";
import type { Prepared, PreviewInputs } from "./preview";

const inputs = (key: string, overrides: Partial<PreviewInputs> = {}): PreviewInputs => ({
  key, retryKey: `retry-${key}`, contextReady: true,
  ask: { selection: [{ id: { library: "synthetic", key }, cite: key, locator: null }], output: "pandoc", revision: "1" },
  ...overrides,
});
const filePreview = (text: string, problem: string | null = null) => ({ text, ticket: `ticket-${text}`, content_hash: "hash", problem } as unknown as FilePreview);
const prepared = (text: string): Prepared => ({ markup: [text], text, html: "", warnings: [], spans: [], fileTicket: `ticket-${text}`, fileHash: "hash" } as unknown as Prepared);
const previewed = (request: number, text: string): SessionEvent => ({ type: "previewed", request, preview: filePreview(text), value: prepared(text) });
const shown = { type: "shown", now: 1, presentation: 1 } as const;

function run(events: SessionEvent[], start: SessionState = { ...initialSession(), resting: false }) {
  let state = start;
  const effects: SessionEffect[] = [];
  for (const event of events) {
    const [next, asked] = reduceSession(state, event);
    state = next;
    effects.push(...asked);
  }
  return { state, effects };
}
const asks = (effects: SessionEffect[]) => effects.filter((effect) => effect.type === "preview");
const text = (s: SessionState) => {
  const value = s.preview.result?.value;
  return typeof value === "object" ? value.text : value;
};

test("a chosen reference is previewed and its answer is what Enter would paste", () => {
  const { state, effects } = run([shown, { type: "preview-inputs", inputs: inputs("a") }, previewed(1, "Garcia 2020")]);
  expect(asks(effects)).toHaveLength(1);
  expect(text(state)).toBe("Garcia 2020");
  expect(state.preview.result?.request).toBe(state.preview.request);
});

test("an engine problem is a failed preview, and choosing again asks afresh", () => {
  const failed = run([shown, { type: "preview-inputs", inputs: inputs("a") },
    { type: "previewed", request: 1, preview: filePreview("", "The style cannot format this item."), value: prepared("") }]);
  expect(failed.state.preview.result).toMatchObject({ value: "failed", problem: "The style cannot format this item." });
  const again = run([{ type: "preview-inputs", inputs: inputs("b") }, { type: "preview-inputs", inputs: inputs("a") }], failed.state);
  expect(asks(again.effects)).toHaveLength(2);
});

test("a late answer for an earlier choice never replaces the newer one", () => {
  const { state, effects } = run([
    shown,
    { type: "preview-inputs", inputs: inputs("a") },
    { type: "preview-inputs", inputs: inputs("b") },
    previewed(2, "Okafor 2021"),
    previewed(1, "Garcia 2020"),
  ]);
  expect(effects.map((effect) => effect.type)).toEqual(["preview", "stop-preview", "preview"]);
  expect(text(state)).toBe("Okafor 2021");
});

test("going back to a choice whose answer nothing replaced adopts it without asking", () => {
  const { state, effects } = run([
    shown,
    { type: "preview-inputs", inputs: inputs("a") }, previewed(1, "Garcia 2020"),
    { type: "preview-inputs", inputs: inputs("a", { ask: null }) },
    { type: "preview-inputs", inputs: inputs("a") },
  ]);
  expect(asks(effects)).toHaveLength(1);
  expect(text(state)).toBe("Garcia 2020");
});

test("reopening keeps a settled preview and asks again for one still out", () => {
  const settled = run([shown, { type: "preview-inputs", inputs: inputs("a") }, previewed(1, "Garcia 2020"), shown,
    { type: "preview-inputs", inputs: inputs("a") }]);
  expect(asks(settled.effects)).toHaveLength(1);
  expect(text(settled.state)).toBe("Garcia 2020");

  const unfinished = run([shown, { type: "preview-inputs", inputs: inputs("a") }, shown, { type: "preview-inputs", inputs: inputs("a") },
    previewed(1, "from the earlier opening")]);
  expect(asks(unfinished.effects)).toHaveLength(2);
  expect(unfinished.state.preview.result).toBeNull();
  expect(text(run([previewed(2, "Garcia 2020")], unfinished.state).state)).toBe("Garcia 2020");
});

test("a refusal is asked once more, and a second refusal is said", () => {
  const cancelled = { type: "preview-failed", failure: { kind: "cancelled" } } as const;
  const { state, effects } = run([shown, { type: "preview-inputs", inputs: inputs("a") }, { ...cancelled, request: 1 }, { ...cancelled, request: 2 }]);
  expect(asks(effects)).toHaveLength(2);
  expect(state.preview.result?.value).toBe("failed");
  expect(state.preview.result?.problem).toBeTruthy();
});

test("a refusal while the project context is read waits for its answer, then asks again", () => {
  const waiting = run([shown, { type: "preview-inputs", inputs: inputs("a", { contextReady: false }) },
    { type: "preview-failed", request: 1, failure: { kind: "cancelled" } }]);
  expect(waiting.state.preview.result).toBeNull();
  const answered = run([{ type: "preview-inputs", inputs: inputs("a") }], waiting.state);
  expect(asks(answered.effects)).toHaveLength(1);
});

test("a preview from an older snapshot reads the library again and is not asked again until it changes", () => {
  const stale = run([shown, { type: "preview-inputs", inputs: inputs("a") }, { type: "preview-failed", request: 1, failure: { kind: "stale" } },
    { type: "preview-inputs", inputs: inputs("b") }, { type: "preview-inputs", inputs: inputs("a") }]);
  expect(stale.effects).toContainEqual({ type: "refresh" });
  expect(asks(stale.effects)).toHaveLength(2);
  expect(stale.state.preview.result?.value).toBe("stale");
  const changed = run([{ type: "preview-inputs", inputs: inputs("a2", { retryKey: "retry-a-new-snapshot" }) }], stale.state);
  expect(asks(changed.effects)).toHaveLength(1);
});

test("going back to a choice after an empty result asks again when a newer preview retired its ticket", () => {
  const { effects } = run([
    shown,
    { type: "preview-inputs", inputs: inputs("a") }, previewed(1, "Garcia 2020"),
    // B's native preview retires A's ticket, even though nothing answers it.
    { type: "preview-inputs", inputs: inputs("b") },
    { type: "preview-inputs", inputs: inputs("no results", { ask: null }) },
    { type: "preview-inputs", inputs: inputs("a") },
  ]);
  expect(asks(effects).map((effect) => effect.ask.selection[0]?.cite)).toEqual(["a", "b", "a"]);
});

test("a hidden picker asks the engine nothing, stops the preview still out, and asks again once shown", () => {
  const hidden = run([shown, { type: "preview-inputs", inputs: inputs("a") }, { type: "hidden", now: 10 }, { type: "preview-inputs", inputs: inputs("b") }]);
  expect(hidden.effects.map((effect) => effect.type)).toEqual(["preview", "stop-preview"]);
  const again = run([{ type: "preparing", now: 20 }, shown, { type: "preview-inputs", inputs: inputs("b") }], hidden.state);
  expect(asks(again.effects)).toHaveLength(1);
});
