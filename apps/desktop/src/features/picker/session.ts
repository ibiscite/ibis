// One citation interaction. Pure transitions; I/O belongs to sessionController.
import type { Copied, Locator, SearchResult } from "../../api";
import type { CitedWarning, ClosestMatch, DeliveryOutcome } from "../../bindings";
import { noticeTitle, readDelivered } from "../../copyFeedback";
import type { FormattedCitation } from "../../format/outputs";
import { problemText, say } from "../../i18n";
import { choiceKey, idKey, type CitationDetailsChange, type CitationReference } from "./choice";
import type { OutputId } from "../../format/outputs";
import { closestMatches, closestMatchWarnings } from "./rows";
import { initialSearch, reduceSearch, type SearchEffect, type SearchEvent, type SearchState } from "./search";
import { initialPreview, reducePreview, type PreviewEffect, type PreviewEvent, type PreviewState } from "./preview";
import { deriveView, type PickerInputs } from "./view";
import { initialDestination, writingVersion, reduceDestination, sameOutput, type DestinationEffect, type DestinationEvent, type DestinationState, type OutputToken } from "./destination";

export type { OutputToken };
type Prepared = FormattedCitation & { fileTicket?: string; fileHash?: string };
export type SessionView = {
  appId?: string | null;
  writingId?: string | null;
  /** Native activation and checked-read identity; retires old flights, while
   * a fresh Enter may still wait for the new checked preview. */
  writingVersion?: string | null;
  queryKey: string; format: OutputId; snapshotRevision: string | undefined; defaultChoice: string;
  /** The last offered citation, independent of the search's current rows. */
  offered: CitationReference[];
  chosen: CitationReference[]; shown: Prepared | "failed" | "stale" | null;
  searchPending: boolean; ready: boolean; saving: boolean; shouldAsk: boolean; output: OutputToken;
  /** Recent Citations is on: only then is an app's last citation kept to change. */
  remembersLast: boolean;
};
type Pending = { keepOrder: boolean; choice: string | null; selection: SessionState["selected"]; queryKey: string;
  format: OutputId | null; output: OutputToken; question?: "open" | "answered" };
type Flight = { keepOrder: boolean; id: number; session: number; choice: string; format: OutputId; writingVersion?: string | null; warnings: CitedWarning[];
  shelf: CitationReference[]; replaces: readonly string[] | null; appId: string | null; writingId?: string | null;
  owner: { context: number; session: number; visibility: number; output: OutputToken } };
/** `earlier`: texts a change replaced only on the clipboard, so either may be in the document. */
export type LastCitation = { text: string; earlier?: readonly string[]; references: CitationReference[]; format: OutputId; keepOrder: boolean; project?: string };
export type RemovedReference = { token: number; reference: CitationReference; before: string[]; after: string[] };
export type ShelfCopied = Copied & { closestMatches?: readonly CitedWarning[] };
export type SessionState = {
  keepOrder: boolean; query: string; interpretLocators: boolean; active: number; selected: Map<string, CitationReference>; selectionChanged: boolean;
  /** Advances when a window is about to appear (preparing or shown): what its first frame may show. */
  opened: number;
  /** The query is text selected in the document; it lasts one opening, unless the person edits it. */
  fromDocument: boolean;
  /** The newest citation per app, in memory only: what Change Last Citation reopens. */
  lastCitations: ReadonlyMap<string, LastCitation>;
  /** The shelf is changing this citation, by any text it may have in the document: Enter may replace only that, selected. */
  changing: readonly string[] | null;
  session: number; presentation: number | null; visibility: number; interaction: number; context: number;
  /** What Enter acts on, worked out after every event; null until the outside inputs are known. */
  view: SessionView | null;
  /** Preferences and the library's lifecycle, as the session last heard them. */
  inputs: PickerInputs | null;
  hiddenAt: number | null; selectAll: number; permissionDetour: boolean;
  /** Whether the picker window is on screen: the one answer every part reads.
   * Unknown counts as hidden, so nothing runs before the first native word. */
  shown: boolean;
  /** Engine work rests until the opening snapshot supplies its presentation. */
  resting: boolean;
  /** The newest search and preview held back while resting, asked for when the
   * same opening wakes; a new opening asks afresh instead. */
  deferred: SessionEffect[];
  /** The search for the current query and its shown answer. */
  search: SearchState;
  /** What Enter would paste for the chosen references. */
  preview: PreviewState;
  /** Where the next citation goes and in which format. */
  destination: DestinationState;
  pendingCopy: Pending | null; flights: Flight[]; activeCopy: number | null; nextCopy: number; lastSuccessfulCopy: number;
  copied: Copied | null; copyError: string | null; lastOutcome: string | null;
  shelfCopied: ShelfCopied | null; copyNotice: Copied | null; removed: RemovedReference | null;
};
type Completion = { id: number; session: number };
export type SessionEvent =
  | SearchEvent
  | PreviewEvent
  | DestinationEvent
  | { type: "picker-inputs"; inputs: PickerInputs }
  /** The screen has committed what the session last said, with every input it had. */
  | { type: "committed" }
  | { type: "query"; value: string }
  | { type: "keep-order"; value: boolean }
  | { type: "literal" | "escape-search" | "cancel" | "retire" | "sample" | "change-last" | "forget-last" }
  | { type: "dismiss"; copied?: Copied }
  | { type: "highlight"; index: number; cancel?: boolean }
  | { type: "toggle"; result?: SearchResult; locator?: Readonly<Locator> | null; prefix?: string }
  | { type: "undo-reference" | "expire-removal"; removal: RemovedReference }
  | { type: "remove-reference"; reference: CitationReference }
  /** This app's reference list: its references, each as its entry, then delivered as Enter would. */
  | { type: "reference-list"; references: readonly SearchResult[] }
  /** Move one reference a place along `shown`, the line's order, which a style may have sorted. */
  | { type: "move-reference"; reference: CitationReference; shown: readonly string[]; step: -1 | 1 }
  | { type: "reference"; reference: CitationReference; change: CitationDetailsChange; selectedOnly?: boolean; initialSelection?: readonly CitationReference[] }
  | { type: "enter"; choice: string | null; output?: OutputToken }
  | { type: "question"; format?: OutputId; output?: OutputToken; answer: "answered" | "cancel" }
  | { type: "hidden"; now: number }
  | { type: "preparing"; now: number; selection?: string | null }
  | { type: "shown"; now: number; presentation: number }
  | { type: "hiding" }
  | (Completion & { type: "completed"; outcome: DeliveryOutcome })
  | (Completion & { type: "copy-failed"; message: string });
export type SessionEffect =
  | (Completion & { type: "copy"; ticket: string; hash: string; replaces: readonly string[] | null; presentation: number; closestMatches: ClosestMatch[] })
  | { type: "consume"; output: OutputToken }
  | { type: "refresh" }
  | SearchEffect
  | PreviewEffect
  | DestinationEffect;

export function initialSession(query = "", copied: Copied | null = null): SessionState {
  return { keepOrder: false, query, interpretLocators: true, active: 0, selected: new Map(), selectionChanged: false, fromDocument: false, lastCitations: new Map(), changing: null,
    opened: 0, session: 0, presentation: null, visibility: 0, interaction: 0, context: 0, view: null, inputs: null,
    hiddenAt: null, selectAll: 0, permissionDetour: false, shown: false, resting: true, deferred: [], search: initialSearch, preview: initialPreview, destination: initialDestination,
    pendingCopy: null, flights: [], activeCopy: null, nextCopy: 0, lastSuccessfulCopy: 0, copied, shelfCopied: null, copyNotice: copied, removed: null, copyError: null, lastOutcome: null };
}
export const sessionQueryKey = (state: SessionState) => JSON.stringify([state.query, state.interpretLocators]);
const ownsClipboard = (s: SessionState, f: Flight) => s.activeCopy === f.id && s.context === f.owner.context && s.session === f.owner.session;
const ownsInteraction = (s: SessionState, f: Flight) => ownsClipboard(s, f) && s.view?.output.delivery === f.owner.output.delivery;
const ownsCompletion = (s: SessionState, f: Flight, output: OutputToken) => ownsInteraction(s, f) && s.visibility === f.owner.visibility && sameOutput(f.owner.output, output);
/** Apps whose last citation stays in memory; the oldest goes first. */
const LAST_CITATIONS = 8;
/** Kept only with Recent Citations on and for a known app, so one app's citation is never offered in another. */
const remembered = (last: ReadonlyMap<string, LastCitation>, app: string | null | undefined, citation: LastCitation, on: boolean) => {
  if (!on || !app) return last;
  const next = new Map(last);
  next.delete(app);
  next.set(app, citation);
  if (next.size > LAST_CITATIONS) next.delete(next.keys().next().value!);
  return next;
};
/** After a minute away the search starts clean. Document text never returns. */
const returning = (s: SessionState, now: number): SessionState => {
  // A change never outlives its opening, unless a permission detour brings it back.
  if (s.hiddenAt !== null && s.changing !== null && !s.permissionDetour) s = changed(s, { selected: new Map(), removed: null, changing: null, keepOrder: false });
  const expired = !s.permissionDetour && s.hiddenAt !== null && now - s.hiddenAt > 60_000;
  return expired ? changed(s, { view: null, query: "", interpretLocators: true, fromDocument: false, selected: new Map(), removed: null, active: 0, keepOrder: false }) :
    s.fromDocument && s.hiddenAt !== null && !s.permissionDetour ? changed(s, { query: "", interpretLocators: true, fromDocument: false, active: 0 }) : s;
};

const changed = (s: SessionState, change: Partial<SessionState>): SessionState => ({ ...s, shelfCopied: null, copyNotice: null, ...change, pendingCopy: null, interaction: s.interaction + 1, context: s.context + 1 });
const finish = (s: SessionState, f: Flight): SessionState => ({ ...s, flights: s.flights.filter(({ id }) => id !== f.id), activeCopy: s.activeCopy === f.id ? null : s.activeCopy });

// A waiting Enter goes on once the view is ready. It copies only on a person's
// own action or once the screen has committed: an engine answer can arrive
// while newer preferences or library status are still on their way from React.
function advance(s: SessionState, copies: boolean): [SessionState, SessionEffect[]] {
  const p = s.pendingCopy, v = s.view;
  if (!p || !v) return [s, []];
  // The snapshot supplies the presentation before the window can accept Enter.
  const presentation = s.presentation;
  if (presentation === null) return [s, []];
  const key = choiceKey(v.chosen);
  // While the format question is open, a new format or token is its answer arriving.
  const answering = p.question === "open";
  if (p.keepOrder !== s.keepOrder || p.selection !== s.selected || p.queryKey !== sessionQueryKey(s) ||
    !answering && (p.format !== null && p.format !== v.format || !sameOutput(p.output, v.output))) return [{ ...s, pendingCopy: null }, []];
  // A destination read can refresh recents and temporarily retire the current
  // rows. Keep the captured choice until the reply can confirm or reject it.
  if (p.choice !== null && p.choice !== key) return v.searchPending ? [s, []] : [{ ...s, pendingCopy: null }, []];
  if (p.choice === null) {
    if (v.searchPending) return [s, []];
    // The kept shelf is available to a fresh Enter after a miss. It is not an
    // answer to Enter queued for the running search.
    if (!v.chosen.length || s.selected.size === 0 && v.defaultChoice === choiceKey([])) return [{ ...s, pendingCopy: null }, []];
    return advance({ ...s, pendingCopy: { ...p, choice: key } }, copies);
  }
  if (!v.ready || v.saving) return [s, []];
  if (!p.question && v.shouldAsk) return [{ ...s, pendingCopy: { ...p, question: "open" } }, []];
  if (p.question === "open" || v.shown === null || !copies) return [s, []];
  s = { ...s, pendingCopy: null };
  if (!v.chosen.length || v.shown === "failed") return [s, []];
  if (v.shown === "stale" || !v.chosen[0]?.revision) return [{ ...s, copyError: say("picker-snapshot-changed") }, [{ type: "refresh" }]];
  if (v.shown.project_refusal) return [{ ...s, copyError: v.shown.project_refusal }, []];
  if (!v.shown.fileTicket || !v.shown.fileHash) return [{ ...s, copyError: say("picker-preview-error") }, []];
  const owner = { context: s.context, session: s.session, visibility: s.visibility, output: v.output };
  const existing = s.flights.find((f) => f.keepOrder === s.keepOrder && (f.writingVersion ?? null) === (v.writingVersion ?? null) && f.choice === key && f.format === v.format && f.replaces === s.changing && f.owner.output.opening === v.output.opening && f.owner.output.delivery === v.output.delivery && f.owner.visibility === s.visibility && f.owner.session === s.session && f.appId === (v.remembersLast ? v.appId ?? null : null));
  if (existing) return [{ ...s, activeCopy: existing.id, flights: s.flights.map((f) => f === existing ? { ...f, owner } : f) }, []];
  const id = s.nextCopy + 1;
  const flight: Flight = { keepOrder: s.keepOrder, id, session: s.session, choice: key, format: v.format, writingVersion: v.writingVersion, owner, warnings: closestMatchWarnings(v.chosen), shelf: v.chosen, replaces: s.changing, appId: v.remembersLast ? v.appId ?? null : null, writingId: v.writingId };
  return [{ ...s, flights: [...s.flights, flight], activeCopy: id, nextCopy: id, copied: null, shelfCopied: null, copyNotice: null, copyError: null },
    [{ type: "copy", id, session: s.session, ticket: v.shown.fileTicket, hash: v.shown.fileHash, replaces: s.changing, presentation, closestMatches: closestMatches(v.chosen) }]];
}

/** The project context changed: what was copied or asked for under the old one is retired. */
function observeProject(s: SessionState, clearReplacement: boolean): [SessionState, SessionEffect[]] {
  const view = s.view, version = writingVersion(s.destination), output = s.destination.output;
  const changed = view && ((view.writingVersion ?? null) !== version || !sameOutput(view.output, output));
  return [{ ...s, context: s.context + (changed ? 1 : 0),
    changing: clearReplacement ? null : s.changing,
    shelfCopied: changed ? null : s.shelfCopied, copyNotice: changed ? null : s.copyNotice }, []];
}

/** A changed view retires what was made for the old one, then Enter may go on. */
function applyView(s: SessionState, copies: boolean): [SessionState, SessionEffect[]] {
  const v = deriveView(s), old = s.view;
  if (!v) return [s, []];
  const same = old && Object.entries(v).every(([key, value]) => key === "chosen" || key === "offered" ? choiceKey(old[key]) === choiceKey(v[key]) :
    key === "output" ? sameOutput(old.output, v.output) : old[key as keyof SessionView] === value);
  if (same) return advance(s, copies);
  const writingChanged = old && (old.writingId ?? null) !== (v.writingId ?? null);
  const contextChanged = old && (writingChanged || old.writingVersion !== v.writingVersion || old.format !== v.format || old.snapshotRevision !== v.snapshotRevision || s.selected.size === 0 && old.defaultChoice !== v.defaultChoice);
  const newOutput = old && (writingChanged || old.format !== v.format || old.snapshotRevision !== v.snapshotRevision);
  const abandonReplacement = writingChanged || old && v.writingId && (old.format !== v.format || old.output.choice !== v.output.choice);
  if (!v.remembersLast && (s.lastCitations.size || s.changing || s.flights.some((f) => f.appId))) s = reduceEvent(s, { type: "forget-last" })[0];
  return advance({ ...s, view: v, context: s.context + (contextChanged ? 1 : 0),
    changing: abandonReplacement ? null : s.changing,
    shelfCopied: newOutput ? null : s.shelfCopied, copyNotice: newOutput ? null : s.copyNotice }, copies);
}

/** One event, then the view it leaves. */
export function reduceSession(s: SessionState, event: SessionEvent): [SessionState, SessionEffect[]] {
  const copies = event.type === "enter" || event.type === "question" || event.type === "committed";
  const [next, effects] = reduceEvent(s, event);
  if (next === s && event.type !== "committed") return [next, effects];
  const [viewed, more] = applyView(next, copies);
  return rest(s, viewed, [...effects, ...more]);
}

const engineWork = (effect: SessionEffect) => effect.type === "search" || effect.type === "preview";
/** A resting picker asks the engine nothing. It keeps the newest search and
 * preview, and asks for them when the same opening wakes. */
function rest(before: SessionState, s: SessionState, effects: SessionEffect[]): [SessionState, SessionEffect[]] {
  if (s.resting) {
    // A stopped preview, or a newer request, replaces what was held back.
    const stopped = effects.some((effect) => effect.type === "stop-preview");
    if (!stopped && !effects.some(engineWork)) return [s, effects];
    const held = s.deferred.filter((effect) => !effects.some((next) => next.type === effect.type) && !(stopped && effect.type === "preview"));
    const deferred = [...held, ...effects.filter(engineWork)];
    return [{ ...s, deferred }, effects.filter((effect) => !engineWork(effect))];
  }
  if (!before.resting || s.deferred.length === 0) return [s, effects];
  // A new opening's hooks ask again for it; the same opening wakes with what it held back.
  const woken = s.session === before.session ? s.deferred.filter((effect) => !effects.some((next) => next.type === effect.type)) : [];
  return [{ ...s, deferred: [] }, [...woken, ...effects]];
}

function reduceEvent(s: SessionState, event: SessionEvent): [SessionState, SessionEffect[]] {
  switch (event.type) {
    case "preview-inputs": case "previewed": case "preview-failed": {
      const step = reducePreview(s.preview, event, s.session, s.opened);
      return step ? [{ ...s, preview: step.preview }, step.effects] : [s, []];
    }
    case "search-inputs": case "searched": case "search-failed": case "announce": {
      if (event.type === "announce" && s.resting) return [s, []];
      const step = reduceSearch(s.search, event, s.session);
      if (!step) return [s, []];
      const next = { ...s, search: step.search };
      return [step.firstRow ? reduceEvent(next, { type: "highlight", index: 0 })[0] : next, step.effects];
    }
    case "destination-snapshot": case "destination-reader": case "read-destination": case "destination-read": case "destination-read-failed": case "project-changed":
    case "choose-format": case "choose-once": case "choose-paste": case "destination-saved": case "destination-save-failed":
    case "destination-save-settled": case "project-edit": case "project-edit-failed": case "consume-held": {
      const step = reduceDestination(s.destination, event);
      if (!step) return [s, []];
      const next = { ...s, destination: step.destination };
      if (!step.observed) return [next, step.effects];
      const [observed, effects] = observeProject(next, step.observed.clearReplacement);
      return [observed, [...step.effects, ...effects]];
    }
    case "picker-inputs": return [{ ...s, inputs: event.inputs }, []];
    case "committed": return [s, []];
    case "keep-order": return [event.value === s.keepOrder ? s : changed(s, { keepOrder: event.value }), []];
    case "query": return [changed(s, { query: event.value, interpretLocators: true, fromDocument: false }), []];
    // A shown or hidden event since the read started is newer than its answer.
    case "literal": return [changed(s, { interpretLocators: false }), []];
    case "escape-search": return [changed(s, { query: "", interpretLocators: true, fromDocument: false }), []];
    case "sample": return [changed(s, { keepOrder: false, selected: new Map(), copied: null, removed: null, copyError: null, changing: null, lastCitations: new Map() }), []];
    case "change-last": {
      const last = s.lastCitations.get(s.view?.appId ?? "");
      if (!last || (last.project ?? null) !== (s.view?.writingId ?? null)
        || last.project && last.format !== s.view?.format) return [s, []];
      return [changed(s, { selected: new Map(last.references.map((reference) => [idKey(reference.id), reference])), removed: null, selectionChanged: true,
        query: "", interpretLocators: true, fromDocument: false, active: 0, copyError: null, keepOrder: last.keepOrder, changing: [last.text, ...last.earlier ?? []] }), []];
    }
    case "reference-list": {
      if (!event.references.length || s.hiddenAt !== null) return [s, []];
      const references = event.references.map((reference): CitationReference => ({ ...reference, form: "bibliography-only" }));
      const next = changed(s, { selected: new Map(references.map((reference) => [idKey(reference.id), reference])), removed: null, selectionChanged: true,
        query: "", interpretLocators: true, fromDocument: false, active: 0, copyError: null, keepOrder: false, changing: null });
      return reduceEvent(next, { type: "enter", choice: choiceKey(references) });
    }
    case "forget-last": return [{ ...s, lastCitations: new Map(), changing: null, flights: s.flights.map((f) => ({ ...f, appId: null })) }, []];
    case "cancel": return [s.pendingCopy ? { ...s, pendingCopy: null } : s, []];
    case "dismiss": return [s.copied && (!event.copied || event.copied === s.copied) ? { ...s, copied: null, shelfCopied: null, copyNotice: null } : s, []];
    case "highlight": {
      if (event.index === s.active) return event.cancel ? reduceEvent(s, { type: "cancel" }) : [s, []];
      return [{ ...s, active: event.index, shelfCopied: null, copyNotice: null, interaction: s.interaction + 1, context: s.context + (s.selected.size === 0 ? 1 : 0), pendingCopy: event.cancel ? null : s.pendingCopy }, []];
    }
    case "toggle": {
      if (!event.result) return reduceEvent(s, { type: "cancel" });
      const selected = new Map(s.selected), key = idKey(event.result.id);
      const previous = selected.get(key), order = [...selected.keys()], index = order.indexOf(key);
      let removed = s.removed;
      if (previous) {
        selected.delete(key);
        removed = { token: s.context + 1, reference: previous, before: order.slice(0, index), after: order.slice(index + 1) };
      } else {
        selected.set(key, { ...event.result, locator: event.locator ? { ...event.locator } : null, ...event.prefix ? { prefix: event.prefix } : {} });
        if (removed && idKey(removed.reference.id) === key) removed = null;
      }
      return [changed(s, { selected, removed, selectionChanged: true, changing: selected.size > 0 ? s.changing : null }), []];
    }
    case "remove-reference": {
      if (s.selected.get(idKey(event.reference.id)) !== event.reference) return [s, []];
      return reduceEvent(s, { type: "toggle", result: event.reference });
    }
    case "move-reference": {
      const key = idKey(event.reference.id), order = [...event.shown];
      const index = order.indexOf(key), target = index + event.step;
      // Only the line the person sees now can be moved: a stale order or reference does nothing.
      if (s.selected.get(key) !== event.reference || order.length !== s.selected.size || !order.every((id) => s.selected.has(id))
        || target < 0 || target >= order.length) return [s, []];
      [order[index], order[target]] = [order[target]!, order[index]!];
      // A move would not show in a style that sorts, so it keeps the person's order.
      return [changed(s, { selected: new Map(order.map((id) => [id, s.selected.get(id)!])), keepOrder: true }), []];
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
      if (event.selectedOnly ? previous !== event.reference : previous ? previous !== event.reference : s.selected.size > 0) return reduceEvent(s, { type: "cancel" });
      const selected = new Map(s.selected);
      if (selected.size === 0) for (const reference of event.initialSelection ?? []) selected.set(idKey(reference.id), reference);
      selected.set(key, { ...event.reference, ...event.change });
      return [changed(s, { selected, selectionChanged: event.selectedOnly ? s.selectionChanged : true }), []];
    }
    case "enter": {
      if (s.hiddenAt !== null || !s.view) return [s, []];
      const output = event.output ?? s.view.output;
      const next = { ...s, pendingCopy: { keepOrder: s.keepOrder, choice: event.choice, selection: s.selected, queryKey: sessionQueryKey(s),
        format: s.view.ready && sameOutput(s.view.output, output) ? s.view.format : null, output } };
      return [next, []];
    }
    case "question": {
      const p = s.pendingCopy;
      if (p?.question !== "open" || event.answer === "cancel") return reduceEvent(s, { type: "cancel" });
      return [{ ...s, pendingCopy: { ...p, question: event.answer, format: event.format === undefined ? p.format : event.format, output: event.output ?? p.output } }, []];
    }
    case "hiding": return [{ ...s, visibility: s.visibility + 1, pendingCopy: null, shelfCopied: null, copyNotice: null }, []];
    case "hidden": {
      // Leaving a change abandons it, so the shortcut alone always starts a new
      // search. Only the change's own copy keeps it until the outcome, and a
      // permission detour is the same change coming back.
      const copying = s.flights.some((f) => f.id === s.activeCopy && f.replaces === s.changing);
      const left = s.changing !== null && !s.permissionDetour && !copying ? { selected: new Map(), changing: null, keepOrder: false } : {};
      // The preview still out answers nothing now; showing again asks afresh.
      return [{ ...s, ...left, presentation: null, visibility: s.visibility + 1, hiddenAt: event.now, pendingCopy: null, shelfCopied: null, copyNotice: null,
        shown: false, resting: true, preview: { ...s.preview, started: null } }, s.preview.started ? [{ type: "stop-preview" }] : []];
    }
    case "preparing": {
      s = { ...returning(s, event.now), hiddenAt: null, opened: s.opened + 1, view: null,
        search: { ...initialSearch, seq: s.search.seq + 1 }, preview: { ...initialPreview, request: s.preview.request + 1 } };
      if (!event.selection) return [s, []];
      // The paste would replace this text, so it always shows, even after a
      // permission detour; the shelf stays.
      return [changed(s, { query: event.selection, interpretLocators: true, fromDocument: true, active: 0, selectAll: s.selectAll + 1, permissionDetour: false }), []];
    }
    case "shown": {
      s = returning(s, event.now);
      return [{ ...s, opened: s.opened + 1, session: s.session + 1, presentation: event.presentation, visibility: s.visibility + 1, hiddenAt: null, pendingCopy: null, shelfCopied: null, copyNotice: null,
        shown: true, resting: false }, []];
    }
    case "retire": return [{ ...s, presentation: null, session: s.session + 1, visibility: s.visibility + 1, pendingCopy: null, flights: [], activeCopy: null }, []];
  }
  const f = s.flights.find((flight) => flight.id === event.id && flight.session === event.session);
  if (!f) return [s, []];
  // The destination's token now: a newer format, paste choice or opening owns the feedback.
  const live = s.destination.output;
  const complete = (state: SessionState): [SessionState, SessionEffect[]] => {
    if (!ownsInteraction(state, f)) return [finish(state, f), []];
    return [finish(changed(state, { selected: new Map(), removed: null, shelfCopied: state.shelfCopied, copyNotice: state.copyNotice, permissionDetour: false, changing: null, keepOrder: false }), f), [{ type: "consume", output: f.owner.output }]];
  };
  if (event.type === "copy-failed") return [finish(ownsCompletion(s, f, live) ? { ...s, copyError: event.message } : s, f), []];
  const result = event.outcome;
  if (result.outcome === "cancelled") return [finish(s, f), []];
  if (result.outcome === "refused") {
    const stale = ["snapshot_changed", "changed-item", "missing-item"].includes(result.problem.code);
    return [finish(ownsCompletion(s, f, live) ? { ...s, copyError: problemText(result.problem, say("picker-copy-error")) } : s, f), stale ? [{ type: "refresh" }] : []];
  }
  const outcome = result.copied;
  const delivered = result.outcome === "pasted" ? { outcome: "pasted" as const, app: result.app }
    : result.outcome === "copied-only" ? { outcome: "copied" as const, reason: result.reason } : null;
  if (f.id > s.lastSuccessfulCopy) s = { ...s, lastSuccessfulCopy: f.id,
    lastOutcome: delivered ? noticeTitle({ delivered: readDelivered(delivered) }) : say("copied-notice-title"),
    lastCitations: remembered(s.lastCitations, f.appId, { text: outcome.text, references: f.shelf, format: f.format, keepOrder: f.keepOrder, ...f.writingId ? { project: f.writingId } : {} }, s.view?.remembersLast === true) };
  const last = f.appId === null ? undefined : s.lastCitations.get(f.appId);
  if (last && f.replaces !== null && f.id === s.lastSuccessfulCopy && delivered?.outcome === "copied" && delivered.reason === "citation-not-selected") {
    const earlier = f.replaces.filter((text) => text !== last.text);
    s = { ...s, lastCitations: new Map(s.lastCitations).set(f.appId!, { ...last, earlier }) };
  }
  const sameVisibleOutput = s.view?.defaultChoice === f.choice && s.view.shown && typeof s.view.shown === "object" && s.view.shown.text === outcome.text;
  // A delivery policy revoked after the write still confirms that copy in place.
  if (!s.pendingCopy && ownsClipboard(s, f) && s.visibility === f.owner.visibility && f.owner.output.opening === live.opening
    && f.owner.output.choice === live.choice && f.owner.output.delivery !== live.delivery) {
    const copied = { text: outcome.text, warnings: outcome.warnings };
    return [finish(changed(s, { selected: new Map(), removed: null, permissionDetour: false, changing: null, keepOrder: false,
      copied, shelfCopied: sameVisibleOutput ? { ...copied, closestMatches: f.warnings } : null,
      copyNotice: sameVisibleOutput ? null : copied }), f), [{ type: "consume", output: live }]];
  }
  // Native hiding is part of the operation. Its hidden event may arrive before
  // this reply; a new opening or interaction still owns its own feedback.
  if (!ownsInteraction(s, f) || !sameOutput(f.owner.output, live)) {
    return [finish(s, f), !s.pendingCopy && s.activeCopy === f.id && f.owner.session === s.session && sameOutput(f.owner.output, live)
      ? [{ type: "consume", output: f.owner.output }] : []];
  }
  if (result.outcome === "retained" && result.reason === "presentation-changed" || result.outcome === "sent" && result.retained === "presentation-changed") return [finish(s, f), []];
  s = { ...s, copied: outcome, shelfCopied: sameVisibleOutput ? { ...outcome, closestMatches: f.warnings } : null,
    copyNotice: sameVisibleOutput ? null : outcome };
  const hideFailed = result.outcome === "retained" && result.reason === "hide-failed" || result.outcome === "sent" && result.retained === "hide-failed";
  const effects: SessionEffect[] = hideFailed || delivered || result.outcome === "sent" && result.retained === null
    ? [{ type: "report", operation: "hide", failed: hideFailed }] : [];
  const permissionRefused = delivered?.outcome === "copied" && (delivered.reason === "no-permission" || delivered.reason === "accessibility-required");
  if (permissionRefused) return [finish({ ...s, permissionDetour: true }, f), effects];
  const [next, more] = complete(s);
  return [next, [...effects, ...more]];
}
