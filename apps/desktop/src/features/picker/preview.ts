// What Enter would paste for the chosen references, as part of the picker
// session (decision 0035): one numbered preview request at a time, and only the
// newest started one may answer.
import type * as nativeApi from "../../api";
import type { FilePreview } from "../../bindings";
import type { FormattedCitation, OutputId } from "../../format/outputs";
import { say } from "../../i18n";

export type Prepared = FormattedCitation & { fileTicket?: string; fileHash?: string };
type Selection = Parameters<typeof nativeApi.previewFileCitation>[0][number];

/** One preview to ask for: the chosen references in one output on one snapshot. */
export type PreviewAsk = { selection: Selection[]; output: OutputId; revision: string; projectContext?: [number, number] | null };

export type PreviewInputs = {
  /** The output, library version, choice and project context. A preview is tagged with it. */
  key: string;
  /** What decides a stale retry: the same key on the same snapshot. */
  retryKey: string;
  /** Null when nothing is chosen or its snapshot is not known: nothing is asked. */
  ask: PreviewAsk | null;
  /** False while the destination, and with it the project context, is being read. */
  contextReady: boolean;
};

export type PreviewResult = {
  key: string; request: number; session: number; opened: number;
  preview?: FilePreview; value: Prepared | "failed" | "stale"; problem?: string;
};

export type PreviewState = {
  inputs: PreviewInputs | null; session: number;
  /** The current request: a new one for every new key, and for every ask again. */
  request: number; key: string | null;
  /** The request last sent to the engine, with its opening: only it may answer. */
  started: { request: number; session: number; key: string; retryKey: string } | null;
  result: PreviewResult | null;
  /** A retry key whose preview met a newer snapshot: not asked again until the snapshot changes. */
  staleKey: string | null;
  /** A refused preview is asked for once more per opening and key. */
  askedAgain: string | null;
  /** A refusal while the context was still being read waits for its answer. */
  waiting: string | null;
};

export const initialPreview: PreviewState = {
  inputs: null, session: -1, request: 0, key: null, started: null, result: null, staleKey: null, askedAgain: null, waiting: null,
};

export type PreviewFailure = { kind: "loading" | "cancelled" | "stale" | "failed"; problem?: string };

export type PreviewEvent =
  | { type: "preview-inputs"; inputs: PreviewInputs }
  | { type: "previewed"; request: number; preview: FilePreview; value: Prepared }
  | { type: "preview-failed"; request: number; failure: PreviewFailure };

export type PreviewEffect = { type: "preview"; request: number; ask: PreviewAsk } | { type: "stop-preview" };

export type PreviewStep = { preview: PreviewState; effects: (PreviewEffect | { type: "refresh" })[] };

const once = (session: number, key: string) => `${session}\u0002${key}`;

/** Start the current request, stopping any other still out. */
function start(s: PreviewState, inputs: PreviewInputs, session: number): PreviewStep {
  return { preview: { ...s, started: { request: s.request, session, key: inputs.key, retryKey: inputs.retryKey } },
    effects: [{ type: "preview", request: s.request, ask: inputs.ask! }] };
}

/** One preview event for the opening `session` (`opened` advances before a reopened window's first frame). */
export function reducePreview(s: PreviewState, event: PreviewEvent, session: number, opened: number): PreviewStep | null {
  switch (event.type) {
    case "preview-inputs": {
      const { inputs } = event;
      const contextAnswered = !s.inputs?.contextReady && inputs.contextReady;
      const sameRequest = inputs.key === s.key && session === s.session;
      // The context's answer may keep the same key, so the refused preview is asked again.
      if (sameRequest && !(contextAnswered && s.waiting === once(session, inputs.key))) {
        return inputs === s.inputs ? null : { preview: { ...s, inputs }, effects: [] };
      }
      let next: PreviewState = { ...s, inputs, session, waiting: contextAnswered ? null : s.waiting };
      if (sameRequest) return start({ ...next, request: s.request + 1, askedAgain: null }, inputs, session);
      // A -> B -> A is a new request: B retires A's native ticket.
      const fresh = inputs.key !== s.key;
      const failed = s.result?.value === "failed";
      // A settled preview survives reopening, including a permission detour;
      // only unfinished or failed work needs the new opening's request.
      const survives = !fresh && Boolean(s.result) && !failed && s.result!.request === s.request;
      if (!survives) next = { ...next, request: s.request + 1, key: inputs.key };
      // Whatever else is out answers nothing now.
      const stop: PreviewStep["effects"] = s.started ? [{ type: "stop-preview" }] : [];
      // An empty search or reconciliation asks for nothing and keeps the settled preview.
      if (!inputs.ask) return { preview: { ...next, started: null }, effects: stop };
      // Back to a key whose preview nothing newer replaced: adopt it without asking.
      if (s.result && !failed && s.result.key === inputs.key && (s.started === null || s.started.request === s.result.request)) {
        return { preview: { ...next, result: { ...s.result, request: next.request } }, effects: [] };
      }
      if (survives) return { preview: next, effects: [] };
      if (s.staleKey === inputs.retryKey) {
        return { preview: { ...next, started: null, result: { key: inputs.key, request: next.request, session, opened, value: "stale" } }, effects: stop };
      }
      const step = start(next, inputs, session);
      return { preview: step.preview, effects: [...stop, ...step.effects] };
    }
    case "previewed": {
      const started = s.started;
      if (!started || started.request !== event.request || started.session !== session) return null;
      return { preview: { ...s, started: null, askedAgain: null, result: { key: started.key, request: started.request, session, opened, preview: event.preview,
        value: event.preview.problem ? "failed" : event.value, problem: event.preview.problem ?? undefined } }, effects: [] };
    }
    case "preview-failed": {
      const started = s.started;
      if (!started || started.request !== event.request || started.session !== session) return null;
      const { failure } = event;
      // The library is opening: its own update brings a fresh request.
      if (failure.kind === "loading") return { preview: { ...s, started: null }, effects: [] };
      const result = (value: PreviewResult["value"], problem?: string): PreviewResult =>
        ({ key: started.key, request: started.request, session, opened, value, ...problem ? { problem } : {} });
      if (failure.kind === "cancelled") {
        // The engine also says "cancelled" when it refuses the project context
        // it was asked under. While that context is being read, its answer asks
        // again. Otherwise ask once more; a second refusal is said, never
        // "Formatting…" forever.
        const again = once(session, started.key);
        if (!s.inputs?.contextReady) return { preview: { ...s, started: null, waiting: again }, effects: [] };
        if (s.askedAgain !== again && s.inputs.ask) return start({ ...s, request: s.request + 1, askedAgain: again }, s.inputs, session);
        return { preview: { ...s, started: null, result: result("failed", say("picker-preview-refused")) }, effects: [] };
      }
      if (failure.kind === "stale") {
        return { preview: { ...s, started: null, staleKey: started.retryKey, result: result("stale") }, effects: [{ type: "refresh" }] };
      }
      return { preview: { ...s, started: null, result: result("failed", failure.problem) }, effects: [] };
    }
  }
}
