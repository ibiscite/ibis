// One citation interaction. Pure transitions; I/O belongs to sessionController.
import type { Copied, Locator, SearchResult } from "../../api";
import type { Delivered } from "../../bindings";
import { afterCopy, noticeTitle } from "../../copyFeedback";
import type { FormattedCitation } from "../../format/styles";
import { copiedNoticeText, pickerText } from "../../messages";
import { choiceKey, idKey, type Choice, type CitationReference } from "./choice";
import { closestMatchWarnings } from "./rows";

export type OutputToken = { opening: number; choice: number };
type Prepared = FormattedCitation & { fileTicket?: string; fileHash?: string };
export type SessionView = {
  queryKey: string; format: Choice; snapshotRevision: string | undefined; defaultChoice: string;
  chosen: CitationReference[]; shown: Prepared | "failed" | "stale" | null;
  searchPending: boolean; ready: boolean; saving: boolean; shouldAsk: boolean; output: OutputToken;
};
type Pending = { choice: string | null; selection: SessionState["selected"]; queryKey: string;
  format: Choice | null; output: OutputToken; question?: "open" | "answered" };
type Flight = { id: number; session: number; presentation: number; choice: string; format: Choice; opening: number; warnings: string[];
  phase: "copy" | "hide" | "notice"; outcome?: Copied;
  owner: { context: number; session: number; visibility: number; output: OutputToken } };
export type SessionState = {
  query: string; interpretLocators: boolean; active: number; selected: Map<string, CitationReference>; selectionChanged: boolean;
  session: number; presentation: number; visibility: number; interaction: number; context: number; viewInteraction: number; view: SessionView | null;
  hiddenAt: number | null; escapedQuery: { text: string; interpretLocators: boolean } | null; restored: number; permissionDetour: boolean;
  pendingCopy: Pending | null; flights: Flight[]; activeCopy: number | null; nextCopy: number; lastSuccessfulCopy: number;
  copied: Copied | null; copyError: string | null; lastOutcome: string | null;
};
type Completion = { id: number; session: number };
export type SessionEvent =
  | { type: "view"; session: number; interaction: number; view: SessionView }
  | { type: "query"; value: string }
  | { type: "literal" | "escape-search" | "cancel" | "retire" | "sample" }
  | { type: "dismiss"; copied?: Copied }
  | { type: "highlight"; index: number; cancel?: boolean }
  | { type: "toggle"; result?: SearchResult; locator?: Readonly<Locator> | null }
  | { type: "reference"; reference: CitationReference; change: Pick<CitationReference, "locator" | "form">; selectedOnly?: boolean }
  | { type: "enter"; choice: string | null; output?: OutputToken }
  | { type: "question"; format?: Choice | null; output?: OutputToken; answer: "open" | "answered" | "cancel" }
  | { type: "hidden"; now: number }
  | { type: "shown"; now: number; presentation: number }
  | { type: "presentation"; presentation: number }
  | { type: "hiding" }
  | (Completion & { type: "copied"; outcome: Copied; output: OutputToken; pinned: boolean })
  | (Completion & { type: "copy-failed"; output: OutputToken; message: string; stale: boolean })
  | (Completion & { type: "hid" | "hide-failed" | "hide-refused" })
  | (Completion & { type: "delivered"; delivered: Delivered | null });
export type SessionEffect =
  | (Completion & { type: "copy"; ticket: string; hash: string })
  | (Completion & { type: "hide"; presentation: number })
  | (Completion & { type: "notice"; outcome: Copied })
  | { type: "hide-status"; failed: boolean }
  | { type: "consume"; output: OutputToken }
  | { type: "refresh" | "ask-format" };

export function initialSession(query = "", copied: Copied | null = null): SessionState {
  return { query, interpretLocators: true, active: 0, selected: new Map(), selectionChanged: false,
    session: 0, presentation: 0, visibility: 0, interaction: 0, context: 0, viewInteraction: -1, view: null,
    hiddenAt: null, escapedQuery: null, restored: 0, permissionDetour: false,
    pendingCopy: null, flights: [], activeCopy: null, nextCopy: 0, lastSuccessfulCopy: 0, copied, copyError: null, lastOutcome: null };
}
export const sessionQueryKey = (state: SessionState) => JSON.stringify([state.query, state.interpretLocators]);
const sameOutput = (a: OutputToken, b: OutputToken) => a.opening === b.opening && a.choice === b.choice;
const ownsInteraction = (s: SessionState, f: Flight) => s.activeCopy === f.id && s.context === f.owner.context && s.session === f.owner.session;
const ownsCompletion = (s: SessionState, f: Flight, output: OutputToken) => ownsInteraction(s, f) && s.visibility === f.owner.visibility && sameOutput(f.owner.output, output);
const changed = (s: SessionState, change: Partial<SessionState>): SessionState => ({ ...s, ...change, pendingCopy: null, interaction: s.interaction + 1, context: s.context + 1 });
const finish = (s: SessionState, f: Flight): SessionState => ({ ...s, flights: s.flights.filter(({ id }) => id !== f.id), activeCopy: s.activeCopy === f.id ? null : s.activeCopy });

// Readiness is supplied by the search/preview adapters. Their captured session
// and interaction must still match before they can release a waiting Enter.
function advance(s: SessionState): [SessionState, SessionEffect[]] {
  const p = s.pendingCopy, v = s.view;
  if (!p || !v || s.viewInteraction !== s.interaction) return [s, []];
  const key = choiceKey(v.chosen);
  if (p.selection !== s.selected || p.queryKey !== sessionQueryKey(s) || p.choice !== null && p.choice !== key ||
    p.format !== null && p.format !== v.format || !sameOutput(p.output, v.output)) return [{ ...s, pendingCopy: null }, []];
  if (p.choice === null) {
    if (v.searchPending) return [s, []];
    if (!v.chosen.length) return [{ ...s, pendingCopy: null }, []];
    return advance({ ...s, pendingCopy: { ...p, choice: key } });
  }
  if (!v.ready || v.saving) return [s, []];
  if (!p.question && v.shouldAsk) return [{ ...s, pendingCopy: { ...p, question: "open" } }, [{ type: "ask-format" }]];
  if (p.question === "open" || v.shown === null) return [s, []];
  s = { ...s, pendingCopy: null };
  if (!v.chosen.length || v.shown === "failed") return [s, []];
  if (v.shown === "stale" || !v.chosen[0]?.revision) return [{ ...s, copyError: pickerText.snapshotChanged }, [{ type: "refresh" }]];
  if (!v.shown.fileTicket || !v.shown.fileHash) return [{ ...s, copyError: pickerText.previewError }, []];
  const owner = { context: s.context, session: s.session, visibility: s.visibility, output: v.output };
  const existing = s.flights.find((f) => f.choice === key && f.format === v.format && f.opening === v.output.opening && f.owner.visibility === s.visibility && f.owner.session === s.session);
  if (existing) return [{ ...s, activeCopy: existing.id, flights: s.flights.map((f) => f === existing ? { ...f, owner } : f) }, []];
  const id = s.nextCopy + 1;
  const flight: Flight = { id, session: s.session, presentation: s.presentation, choice: key, format: v.format, opening: v.output.opening, owner, phase: "copy", warnings: closestMatchWarnings(v.chosen) };
  return [{ ...s, flights: [...s.flights, flight], activeCopy: id, nextCopy: id, copied: null, copyError: null },
    [{ type: "copy", id, session: s.session, ticket: v.shown.fileTicket, hash: v.shown.fileHash }]];
}

export function reduceSession(s: SessionState, event: SessionEvent): [SessionState, SessionEffect[]] {
  switch (event.type) {
    case "view": {
      if (event.session !== s.session || event.interaction !== s.interaction || event.view.queryKey !== sessionQueryKey(s)) return [s, []];
      const v = event.view, old = s.view;
      const contextChanged = old && (old.format !== v.format || old.snapshotRevision !== v.snapshotRevision || s.selected.size === 0 && old.defaultChoice !== v.defaultChoice);
      const same = old && Object.entries(v).every(([key, value]) => key === "chosen" ? choiceKey(old.chosen) === choiceKey(v.chosen) :
        key === "output" ? sameOutput(old.output, v.output) : old[key as keyof SessionView] === value);
      return advance(same && s.viewInteraction === s.interaction ? s : { ...s, view: v, viewInteraction: s.interaction, context: s.context + (contextChanged ? 1 : 0) });
    }
    case "query": return [changed(s, { query: event.value, interpretLocators: true, escapedQuery: null }), []];
    case "literal": return [changed(s, { interpretLocators: false }), []];
    case "escape-search": return [changed(s, { escapedQuery: { text: s.query, interpretLocators: s.interpretLocators }, query: "", interpretLocators: true }), []];
    case "sample": return [changed(s, { selected: new Map(), copied: null, copyError: null }), []];
    case "cancel": return [s.pendingCopy ? { ...s, pendingCopy: null } : s, []];
    case "dismiss": return [s.copied && (!event.copied || event.copied === s.copied) ? { ...s, copied: null } : s, []];
    case "highlight": {
      if (event.index === s.active) return event.cancel ? reduceSession(s, { type: "cancel" }) : [s, []];
      return [{ ...s, active: event.index, interaction: s.interaction + 1, context: s.context + (s.selected.size === 0 ? 1 : 0), pendingCopy: event.cancel ? null : s.pendingCopy }, []];
    }
    case "toggle": {
      if (!event.result) return reduceSession(s, { type: "cancel" });
      const selected = new Map(s.selected), key = idKey(event.result.id);
      if (!selected.delete(key)) selected.set(key, { ...event.result, locator: event.locator ? { ...event.locator } : null });
      return [changed(s, { selected, selectionChanged: true }), []];
    }
    case "reference": {
      const key = idKey(event.reference.id), previous = s.selected.get(key);
      if (event.selectedOnly ? previous !== event.reference : previous ? previous !== event.reference : s.selected.size > 0) return reduceSession(s, { type: "cancel" });
      const selected = new Map(s.selected);
      selected.set(key, { ...event.reference, ...event.change });
      return [changed(s, { selected, selectionChanged: event.selectedOnly ? s.selectionChanged : true }), []];
    }
    case "enter": {
      if (s.hiddenAt !== null || !s.view) return [s, []];
      const output = event.output ?? s.view.output;
      return advance({ ...s, pendingCopy: { choice: event.choice, selection: s.selected, queryKey: sessionQueryKey(s),
        format: s.view.ready && s.viewInteraction === s.interaction && sameOutput(s.view.output, output) ? s.view.format : null, output } });
    }
    case "question": {
      const p = s.pendingCopy;
      if (p?.question !== "open" || event.answer === "cancel") return reduceSession(s, { type: "cancel" });
      return [{ ...s, viewInteraction: -1, pendingCopy: { ...p, question: event.answer, format: event.format === undefined ? p.format : event.format, output: event.output ?? p.output } }, []];
    }
    case "presentation": return [{ ...s, presentation: event.presentation }, []];
    case "hiding": return [{ ...s, visibility: s.visibility + 1, pendingCopy: null }, []];
    case "hidden": return [{ ...s, visibility: s.visibility + 1, hiddenAt: event.now, pendingCopy: null }, []];
    case "shown": {
      const expired = !s.permissionDetour && s.hiddenAt !== null && event.now - s.hiddenAt > 60_000;
      const escaped = s.hiddenAt !== null ? s.escapedQuery : null;
      s = expired ? changed(s, { query: "", interpretLocators: true, selected: new Map(), active: 0, escapedQuery: null }) :
        escaped ? changed(s, { query: s.query || escaped.text, interpretLocators: escaped.interpretLocators, escapedQuery: null, restored: s.restored + 1 }) : s;
      return [{ ...s, session: s.session + 1, presentation: event.presentation, visibility: s.visibility + 1, viewInteraction: -1, hiddenAt: null, pendingCopy: null }, []];
    }
    case "retire": return [{ ...s, session: s.session + 1, visibility: s.visibility + 1, viewInteraction: -1, pendingCopy: null, flights: [], activeCopy: null }, []];
  }
  const f = s.flights.find((flight) => flight.id === event.id && flight.session === event.session);
  if (!f) return [s, []];
  const complete = (state: SessionState): [SessionState, SessionEffect[]] => {
    if (!ownsInteraction(state, f)) return [finish(state, f), []];
    return [finish(changed(state, { selected: new Map(), permissionDetour: false }), f), [{ type: "consume", output: f.owner.output }]];
  };
  switch (event.type) {
    case "copy-failed": return f.phase !== "copy" ? [s, []] : [finish(ownsCompletion(s, f, event.output) ? { ...s, copyError: event.message } : s, f), event.stale ? [{ type: "refresh" }] : []];
    case "copied": {
      if (f.phase !== "copy") return [s, []];
      const outcome = { ...event.outcome, warnings: [...event.outcome.warnings, ...f.warnings] };
      if (f.id > s.lastSuccessfulCopy) s = { ...s, lastSuccessfulCopy: f.id, lastOutcome: copiedNoticeText.title };
      if (!ownsCompletion(s, f, event.output)) return [finish(s, f), s.activeCopy === f.id && f.owner.session === s.session && sameOutput(f.owner.output, event.output) ? [{ type: "consume", output: f.owner.output }] : []];
      s = { ...s, copied: outcome, escapedQuery: null, flights: s.flights.map((flight) => flight === f ? { ...f, phase: "hide", outcome } : flight) };
      return afterCopy(outcome, event.pinned) === "hide-and-notify" ? [{ ...s, permissionDetour: true }, [{ type: "hide", id: f.id, session: f.session, presentation: f.presentation }]] : complete(s);
    }
    case "hid": {
      if (f.phase !== "hide") return [s, []];
      if (!ownsInteraction(s, f)) return [finish(s, f), []];
      return [{ ...s, flights: s.flights.map((flight) => flight === f ? { ...f, phase: "notice" } : flight) },
        [{ type: "hide-status", failed: false }, { type: "notice", id: f.id, session: f.session, outcome: f.outcome! }]];
    }
    case "hide-failed": {
      if (f.phase !== "hide") return [s, []];
      const [next, effects] = complete(s);
      return [next, ownsInteraction(s, f) ? [{ type: "hide-status", failed: true }, ...effects] : effects];
    }
    case "hide-refused": return [finish(ownsInteraction(s, f) ? { ...s, permissionDetour: false } : s, f), []];
    case "delivered": {
      if (f.phase !== "notice") return [s, []];
      if (event.delivered && f.id === s.lastSuccessfulCopy) s = { ...s, lastOutcome: noticeTitle({ delivered: event.delivered, text: "" }) };
      const permissionRefused = event.delivered?.outcome === "copied"
        && (event.delivered.reason === "no-permission" || event.delivered.reason === "accessibility-required");
      return permissionRefused ? [finish(s, f), []] : complete(s);
    }
  }
}
