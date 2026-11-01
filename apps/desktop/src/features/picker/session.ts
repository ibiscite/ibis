// One citation interaction. Pure transitions; I/O belongs to sessionController.
import type { Copied, Locator, SearchResult } from "../../api";
import type { Delivered } from "../../bindings";
import { afterCopy, noticeTitle } from "../../copyFeedback";
import type { FormattedCitation } from "../../format/styles";
import { copiedNoticeText, pickerText } from "../../messages";
import { choiceKey, idKey, type Choice, type CitationReference } from "./choice";
import { closestMatchWarnings } from "./rows";

export type OutputToken = { opening: number; choice: number; delivery: number };
type Prepared = FormattedCitation & { fileTicket?: string; fileHash?: string };
export type SessionView = {
  appId?: string | null;
  queryKey: string; format: Choice; snapshotRevision: string | undefined; defaultChoice: string;
  chosen: CitationReference[]; shown: Prepared | "failed" | "stale" | null;
  previewVisible: boolean;
  searchPending: boolean; ready: boolean; saving: boolean; shouldAsk: boolean; output: OutputToken;
};
type Pending = { choice: string | null; selection: SessionState["selected"]; queryKey: string;
  format: Choice | null; output: OutputToken; question?: "open" | "answered" };
type Flight = { id: number; session: number; presentation: number; choice: string; format: Choice; opening: number; delivery: number; warnings: string[];
  references: unknown[]; appId: string | null;
  phase: "copy" | "hide" | "notice"; outcome?: Copied;
  owner: { context: number; session: number; visibility: number; output: OutputToken } };
export type RemovedReference = { token: number; reference: CitationReference; before: string[]; after: string[] };
export type ShelfCopied = Copied & { closestMatches?: readonly string[] };
export type SessionState = {
  query: string; interpretLocators: boolean; active: number; selected: Map<string, CitationReference>; selectionChanged: boolean;
  session: number; presentation: number | null; visibility: number; interaction: number; context: number; viewInteraction: number; view: SessionView | null;
  presentationSubscribed: boolean; readingPresentation: boolean; retryPresentation: boolean;
  hiddenAt: number | null; escapedQuery: { text: string; interpretLocators: boolean } | null; restored: number; permissionDetour: boolean;
  pendingCopy: Pending | null; flights: Flight[]; activeCopy: number | null; nextCopy: number; lastSuccessfulCopy: number;
  copied: Copied | null; copyError: string | null; lastOutcome: string | null;
  shelfCopied: ShelfCopied | null; copyNotice: Copied | null; removed: RemovedReference | null;
  recentCopy: { app: string | null; ids: unknown[] } | null;
};
type Completion = { id: number; session: number };
export type SessionEvent =
  | { type: "view"; session: number; interaction: number; view: SessionView }
  | { type: "query"; value: string }
  | { type: "literal" | "escape-search" | "cancel" | "retire" | "sample" }
  | { type: "dismiss"; copied?: Copied }
  | { type: "highlight"; index: number; cancel?: boolean }
  | { type: "toggle"; result?: SearchResult; locator?: Readonly<Locator> | null }
  | { type: "undo-reference" | "expire-removal"; removal: RemovedReference }
  | { type: "remove-reference"; reference: CitationReference }
  | { type: "reference"; reference: CitationReference; change: Pick<CitationReference, "locator" | "form">; selectedOnly?: boolean; initialSelection?: readonly CitationReference[] }
  | { type: "enter"; choice: string | null; output?: OutputToken }
  | { type: "question"; format?: Choice | null; output?: OutputToken; answer: "open" | "answered" | "cancel" }
  | { type: "hidden"; now: number }
  | { type: "preparing"; now: number }
  | { type: "shown"; now: number; presentation: number }
  | { type: "initialize" }
  | { type: "presentation"; visibility: number; presentation: number | null }
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
  | { type: "read-presentation"; visibility: number }
  | { type: "refresh" | "ask-format" };

export function initialSession(query = "", copied: Copied | null = null): SessionState {
  return { query, interpretLocators: true, active: 0, selected: new Map(), selectionChanged: false,
    session: 0, presentation: null, visibility: 0, interaction: 0, context: 0, viewInteraction: -1, view: null,
    presentationSubscribed: false, readingPresentation: false, retryPresentation: false,
    hiddenAt: null, escapedQuery: null, restored: 0, permissionDetour: false,
    pendingCopy: null, flights: [], activeCopy: null, nextCopy: 0, lastSuccessfulCopy: 0, copied, shelfCopied: null, copyNotice: copied, removed: null, recentCopy: null, copyError: null, lastOutcome: null };
}
export const sessionQueryKey = (state: SessionState) => JSON.stringify([state.query, state.interpretLocators]);
const sameOutput = (a: OutputToken, b: OutputToken) => a.opening === b.opening && a.choice === b.choice && a.delivery === b.delivery;
const ownsClipboard = (s: SessionState, f: Flight) => s.activeCopy === f.id && s.context === f.owner.context && s.session === f.owner.session;
const ownsInteraction = (s: SessionState, f: Flight) => ownsClipboard(s, f) && s.view?.output.delivery === f.delivery;
const ownsCompletion = (s: SessionState, f: Flight, output: OutputToken) => ownsInteraction(s, f) && s.visibility === f.owner.visibility && sameOutput(f.owner.output, output);
/** After a minute away the search starts clean; otherwise an escaped query returns. */
const returning = (s: SessionState, now: number): SessionState => {
  const expired = !s.permissionDetour && s.hiddenAt !== null && now - s.hiddenAt > 60_000;
  const escaped = s.hiddenAt !== null ? s.escapedQuery : null;
  return expired ? changed(s, { query: "", interpretLocators: true, selected: new Map(), removed: null, active: 0, escapedQuery: null }) :
    escaped ? changed(s, { query: s.query || escaped.text, interpretLocators: escaped.interpretLocators, escapedQuery: null, restored: s.restored + 1 }) : s;
};

const changed = (s: SessionState, change: Partial<SessionState>): SessionState => ({ ...s, shelfCopied: null, copyNotice: null, ...change, pendingCopy: null, interaction: s.interaction + 1, context: s.context + 1 });
const finish = (s: SessionState, f: Flight): SessionState => ({ ...s, flights: s.flights.filter(({ id }) => id !== f.id), activeCopy: s.activeCopy === f.id ? null : s.activeCopy });

function readPresentation(s: SessionState, retry = false): [SessionState, SessionEffect[]] {
  if (s.presentation !== null || !s.presentationSubscribed || s.hiddenAt !== null) return [s, []];
  if (s.readingPresentation) return retry ? [{ ...s, retryPresentation: true }, []] : [s, []];
  return [{ ...s, readingPresentation: true, retryPresentation: retry }, [{ type: "read-presentation", visibility: s.visibility }]];
}

// Readiness is supplied by the search/preview adapters. Their captured session
// and interaction must still match before they can release a waiting Enter.
function advance(s: SessionState): [SessionState, SessionEffect[]] {
  const p = s.pendingCopy, v = s.view;
  if (!p || !v || s.viewInteraction !== s.interaction) return [s, []];
  // A flight must never freeze an invented presentation identity. Keep Enter
  // pending until initialization or a native show supplies its actual owner.
  const presentation = s.presentation;
  if (presentation === null) return [s, []];
  const key = choiceKey(v.chosen);
  if (p.selection !== s.selected || p.queryKey !== sessionQueryKey(s) ||
    p.format !== null && p.format !== v.format || !sameOutput(p.output, v.output)) return [{ ...s, pendingCopy: null }, []];
  // A destination read can refresh recents and temporarily retire the current
  // rows. Keep the captured choice until the reply can confirm or reject it.
  if (p.choice !== null && p.choice !== key) return v.searchPending ? [s, []] : [{ ...s, pendingCopy: null }, []];
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
  const existing = s.flights.find((f) => f.choice === key && f.format === v.format && f.opening === v.output.opening && f.delivery === v.output.delivery && f.owner.visibility === s.visibility && f.owner.session === s.session);
  if (existing) return [{ ...s, activeCopy: existing.id, flights: s.flights.map((f) => f === existing ? { ...f, owner } : f) }, []];
  const id = s.nextCopy + 1;
  const flight: Flight = { id, session: s.session, presentation, choice: key, format: v.format, opening: v.output.opening, delivery: v.output.delivery, owner, phase: "copy", warnings: closestMatchWarnings(v.chosen), references: v.chosen.filter((r) => !r.sample).map((r) => r.id), appId: v.appId ?? null };
  return [{ ...s, flights: [...s.flights, flight], activeCopy: id, nextCopy: id, copied: null, shelfCopied: null, copyNotice: null, copyError: null },
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
      const newOutput = old && (old.format !== v.format || old.snapshotRevision !== v.snapshotRevision);
      return advance(same && s.viewInteraction === s.interaction ? s : { ...s, view: v, viewInteraction: s.interaction, context: s.context + (contextChanged ? 1 : 0),
        shelfCopied: newOutput || !v.previewVisible ? null : s.shelfCopied, copyNotice: newOutput ? null : s.copyNotice });
    }
    case "query": return [changed(s, { query: event.value, interpretLocators: true, escapedQuery: null }), []];
    case "literal": return [changed(s, { interpretLocators: false }), []];
    case "escape-search": return [changed(s, { escapedQuery: { text: s.query, interpretLocators: s.interpretLocators }, query: "", interpretLocators: true }), []];
    case "sample": return [changed(s, { selected: new Map(), copied: null, removed: null, copyError: null }), []];
    case "cancel": return [s.pendingCopy ? { ...s, pendingCopy: null } : s, []];
    case "dismiss": return [s.copied && (!event.copied || event.copied === s.copied) ? { ...s, copied: null, shelfCopied: null, copyNotice: null } : s, []];
    case "highlight": {
      if (event.index === s.active) return event.cancel ? reduceSession(s, { type: "cancel" }) : [s, []];
      return [{ ...s, active: event.index, shelfCopied: null, copyNotice: null, interaction: s.interaction + 1, context: s.context + (s.selected.size === 0 ? 1 : 0), pendingCopy: event.cancel ? null : s.pendingCopy }, []];
    }
    case "toggle": {
      if (!event.result) return reduceSession(s, { type: "cancel" });
      const selected = new Map(s.selected), key = idKey(event.result.id);
      const previous = selected.get(key), order = [...selected.keys()], index = order.indexOf(key);
      let removed = s.removed;
      if (previous) {
        selected.delete(key);
        removed = { token: s.context + 1, reference: previous, before: order.slice(0, index), after: order.slice(index + 1) };
      } else {
        selected.set(key, { ...event.result, locator: event.locator ? { ...event.locator } : null });
        if (removed && idKey(removed.reference.id) === key) removed = null;
      }
      return [changed(s, { selected, removed, selectionChanged: true }), []];
    }
    case "remove-reference": {
      if (s.selected.get(idKey(event.reference.id)) !== event.reference) return [s, []];
      return reduceSession(s, { type: "toggle", result: event.reference });
    }
    case "expire-removal": return [s.removed === event.removal ? { ...s, removed: null } : s, []];
    case "undo-reference": {
      if (s.removed !== event.removal) return [s, []];
      const { reference, before, after } = event.removal, key = idKey(reference.id);
      if (s.selected.has(key)) return [{ ...s, removed: null }, []];
      const entries = [...s.selected], next = entries.findIndex(([id]) => after.includes(id));
      let previous = -1;
      for (let i = 0; i < entries.length; i++) if (before.includes(entries[i]![0])) previous = i;
      entries.splice(next >= 0 ? next : previous >= 0 ? previous + 1 : entries.length, 0, [key, reference]);
      return [changed(s, { selected: new Map(entries), removed: null, selectionChanged: true }), []];
    }
    case "reference": {
      const key = idKey(event.reference.id), previous = s.selected.get(key);
      if (event.selectedOnly ? previous !== event.reference : previous ? previous !== event.reference : s.selected.size > 0) return reduceSession(s, { type: "cancel" });
      const selected = new Map(s.selected);
      if (selected.size === 0) for (const reference of event.initialSelection ?? []) selected.set(idKey(reference.id), reference);
      selected.set(key, { ...event.reference, ...event.change });
      return [changed(s, { selected, selectionChanged: event.selectedOnly ? s.selectionChanged : true }), []];
    }
    case "enter": {
      if (s.hiddenAt !== null || !s.view) return [s, []];
      const output = event.output ?? s.view.output;
      const next = { ...s, pendingCopy: { choice: event.choice, selection: s.selected, queryKey: sessionQueryKey(s),
        format: s.view.ready && s.viewInteraction === s.interaction && sameOutput(s.view.output, output) ? s.view.format : null, output } };
      return s.presentation === null ? readPresentation(next, true) : advance(next);
    }
    case "question": {
      const p = s.pendingCopy;
      if (p?.question !== "open" || event.answer === "cancel") return reduceSession(s, { type: "cancel" });
      return [{ ...s, viewInteraction: -1, pendingCopy: { ...p, question: event.answer, format: event.format === undefined ? p.format : event.format, output: event.output ?? p.output } }, []];
    }
    case "initialize": return readPresentation({ ...s, presentationSubscribed: true }, s.pendingCopy !== null);
    case "presentation": {
      if (!s.readingPresentation || event.visibility !== s.visibility || s.presentation !== null) return [s, []];
      const retry = s.retryPresentation && s.pendingCopy !== null;
      s = { ...s, readingPresentation: false, retryPresentation: false, presentation: event.presentation };
      if (event.presentation !== null) return advance(s);
      // One recovery read for an explicit action; another Enter can retry a
      // persistent failure. No timer or hidden retry loop owns this work.
      if (retry) return readPresentation(s);
      return [s.pendingCopy ? { ...s, pendingCopy: null, copyError: pickerText.copyError } : s, []];
    }
    case "hiding": return [{ ...s, readingPresentation: false, retryPresentation: false, visibility: s.visibility + 1, pendingCopy: null, shelfCopied: null, copyNotice: null }, []];
    case "hidden": return [{ ...s, presentation: null, readingPresentation: false, retryPresentation: false, visibility: s.visibility + 1, hiddenAt: event.now, pendingCopy: null, shelfCopied: null, copyNotice: null }, []];
    case "preparing": return [{ ...returning(s, event.now), hiddenAt: null }, []];
    case "shown": {
      s = returning(s, event.now);
      return [{ ...s, session: s.session + 1, presentation: event.presentation, readingPresentation: false, retryPresentation: false, visibility: s.visibility + 1, viewInteraction: -1, hiddenAt: null, pendingCopy: null, shelfCopied: null, copyNotice: null }, []];
    }
    case "retire": return [{ ...s, presentation: null, presentationSubscribed: false, readingPresentation: false, retryPresentation: false, session: s.session + 1, visibility: s.visibility + 1, viewInteraction: -1, pendingCopy: null, flights: [], activeCopy: null }, []];
  }
  const f = s.flights.find((flight) => flight.id === event.id && flight.session === event.session);
  if (!f) return [s, []];
  const complete = (state: SessionState): [SessionState, SessionEffect[]] => {
    if (!ownsInteraction(state, f)) return [finish(state, f), []];
    return [finish(changed(state, { selected: new Map(), removed: null, shelfCopied: state.shelfCopied, copyNotice: state.copyNotice, permissionDetour: false }), f), [{ type: "consume", output: f.owner.output }]];
  };
  switch (event.type) {
    case "copy-failed": return f.phase !== "copy" ? [s, []] : [finish(ownsCompletion(s, f, event.output) ? { ...s, copyError: event.message } : s, f), event.stale ? [{ type: "refresh" }] : []];
    case "copied": {
      if (f.phase !== "copy") return [s, []];
      const outcome = { ...event.outcome, warnings: [...event.outcome.warnings, ...f.warnings] };
      if (f.id > s.lastSuccessfulCopy) s = { ...s, lastSuccessfulCopy: f.id, lastOutcome: copiedNoticeText.title, recentCopy: { app: f.appId, ids: f.references } };
      // Consuming a built citation can reveal a different default reference.
      // Confirm on the shelf only when its next target is this exact output.
      const sameVisibleOutput = s.view?.previewVisible && s.view.defaultChoice === f.choice && s.view.shown && typeof s.view.shown === "object" && s.view.shown.text === outcome.text;
      // Delivery can be revoked after the native write but before its response.
      // Confirm that write in place, without hiding or forwarding its old proof.
      // A newer citation, operation or opening still owns its own feedback.
      if (!s.pendingCopy && ownsClipboard(s, f) && s.visibility === f.owner.visibility && f.owner.output.opening === event.output.opening
        && f.owner.output.choice === event.output.choice && f.delivery !== event.output.delivery) {
        const copied = { text: outcome.text, warnings: outcome.warnings };
        return [finish(changed(s, { selected: new Map(), removed: null, permissionDetour: false,
          copied, shelfCopied: sameVisibleOutput ? { ...copied, closestMatches: f.warnings } : null,
          copyNotice: sameVisibleOutput ? null : copied, escapedQuery: null }), f), [{ type: "consume", output: event.output }]];
      }
      if (!ownsCompletion(s, f, event.output)) return [finish(s, f), s.activeCopy === f.id && f.owner.session === s.session && sameOutput(f.owner.output, event.output) ? [{ type: "consume", output: f.owner.output }] : []];
      s = { ...s, copied: outcome, shelfCopied: sameVisibleOutput ? { ...outcome, closestMatches: f.warnings } : null,
        copyNotice: sameVisibleOutput ? null : outcome, escapedQuery: null, flights: s.flights.map((flight) => flight === f ? { ...f, phase: "hide", outcome } : flight) };
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
