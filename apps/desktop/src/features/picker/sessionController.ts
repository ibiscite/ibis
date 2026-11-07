// Thin effect runner. It never decides which interaction owns a completion.
import * as nativeApi from "../../api";
import type { Delivered } from "../../bindings";
import { say } from "../../i18n";
import { parse } from "../../format/markup";
import type { PreviewFailure } from "./preview";
import { initialSession, reduceSession, type OutputToken, type SessionEffect, type SessionEvent } from "./session";

export type SessionPorts = Pick<typeof nativeApi, "copyFileCitation" | "hidePicker" | "pickerPresentation" | "search" | "previewFileCitation"> & Partial<Pick<typeof nativeApi, "showCopiedNotice">> & {
  output: () => OutputToken; pinned: () => boolean; consume: (output: OutputToken) => unknown;
  refresh: () => void; askFormat: () => void; reportHide: (failed: boolean) => void;
  /** Recent citations, the answer to an empty query. */
  recent?: () => ReturnType<typeof nativeApi.search>;
};

export function createSessionController(initial = initialSession()) {
  let state = initial;
  let ports = {} as SessionPorts;
  const listeners = new Set<() => void>();
  /** The preview still out; a newer one, or leaving its opening, stops it. */
  let previewing: AbortController | null = null;
  const dispatch = (event: SessionEvent) => {
    const [next, effects] = reduceSession(state, event);
    if (next !== state) { state = next; for (const notify of listeners) notify(); }
    for (const effect of effects) void run(effect);
  };
  const run = async (effect: SessionEffect) => {
    switch (effect.type) {
      case "read-presentation": {
        let presentation: number | null = null;
        try { presentation = await ports.pickerPresentation(); }
        catch { /* A pending action owns one recovery read, never a timer. */ }
        dispatch({ type: "presentation", visibility: effect.visibility, presentation });
        break;
      }
      case "copy": {
        try {
          const outcome = await ports.copyFileCitation(effect.ticket, effect.hash, effect.replaces);
          dispatch({ type: "copied", id: effect.id, session: effect.session, outcome, output: ports.output(), pinned: ports.pinned() });
        } catch (error) {
          const stale = nativeApi.isSnapshotChanged(error), shared = nativeApi.sharedCitationKey(error);
          dispatch({ type: "copy-failed", id: effect.id, session: effect.session, output: ports.output(), stale,
            message: stale ? say("picker-snapshot-changed") : shared ? say("picker-shared-key", { key: shared }) : say("picker-copy-error") });
        }
        break;
      }
      case "hide":
        try {
          const hidden = await ports.hidePicker(effect.presentation);
          dispatch({ type: hidden === false ? "hide-refused" : "hid", id: effect.id, session: effect.session });
        }
        catch { dispatch({ type: "hide-failed", id: effect.id, session: effect.session }); }
        break;
      case "notice": {
        let delivered: Delivered | null = null;
        try { delivered = await ports.showCopiedNotice?.(effect.outcome.text, effect.outcome.delivery ?? null, effect.outcome.summary ?? null) ?? null; }
        catch { console.error(say("copied-notice-notice-failed")); }
        dispatch({ type: "delivered", id: effect.id, session: effect.session, delivered });
        break;
      }
      case "consume": ports.consume(effect.output); break;
      case "hide-status": ports.reportHide(effect.failed); break;
      case "refresh": ports.refresh(); break;
      case "ask-format": ports.askFormat(); break;
      case "search": {
        const { seq } = effect;
        try {
          const reply = await (effect.recent && ports.recent ? ports.recent() : ports.search(effect.query, undefined, undefined, effect.interpretLocators));
          dispatch({ type: "searched", seq, reply });
        } catch { dispatch({ type: "search-failed", seq }); }
        break;
      }
      case "announce-after": setTimeout(() => dispatch({ type: "announce", seq: effect.seq }), effect.ms); break;
      case "stop-preview": previewing?.abort(); previewing = null; break;
      case "preview": {
        previewing?.abort();
        const abort = previewing = new AbortController();
        const { request, ask } = effect;
        try {
          const preview = await ports.previewFileCitation(ask.selection, ask.output, ask.revision, abort.signal, ask.projectContext);
          dispatch({ type: "previewed", request, preview, value: { markup: preview.html ? parse(preview.html) : [preview.text], text: preview.text, html: preview.html ?? "",
            warnings: preview.warnings, spans: preview.spans, project_refusal: preview.project_refusal, notices: preview.notices, fileTicket: preview.ticket, fileHash: preview.content_hash } });
        } catch (error) {
          dispatch({ type: "preview-failed", request, failure: previewFailure(error) });
        } finally {
          if (previewing === abort) previewing = null;
        }
        break;
      }
    }
  };
  // Each part connects the ports it owns; together they are the session's I/O.
  return { dispatch, connect: (next: Partial<SessionPorts>) => { ports = { ...ports, ...next }; }, getSnapshot: () => state,
    /** The picker is gone: the engine stops what nobody will read. */
    stop: () => { previewing?.abort(); previewing = null; },
    subscribe: (notify: () => void) => { listeners.add(notify); return () => { listeners.delete(notify); }; } };
}

/** What a failed preview means for the person, read from the engine's codes. */
function previewFailure(error: unknown): PreviewFailure {
  if (nativeApi.isLibraryLoading(error)) return { kind: "loading" };
  if (nativeApi.isCancelled(error)) return { kind: "cancelled" };
  if (nativeApi.isSnapshotChanged(error)) return { kind: "stale" };
  const shared = nativeApi.sharedCitationKey(error);
  if (shared) return { kind: "failed", problem: say("picker-shared-key", { key: shared }) };
  const failure = typeof error === "object" && error !== null ? error as { code?: unknown; message?: unknown } : null;
  return { kind: "failed", ...failure?.code === "failed" && typeof failure.message === "string" ? { problem: failure.message } : {} };
}
