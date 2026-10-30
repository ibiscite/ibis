// Thin effect runner. It never decides which interaction owns a completion.
import * as nativeApi from "../../api";
import type { CopyReceipt } from "../../api";
import { copiedNoticeText, pickerText } from "../../messages";
import { initialSession, reduceSession, type OutputToken, type SessionEffect, type SessionEvent } from "./session";

export type SessionPorts = Pick<typeof nativeApi, "copyFileCitation" | "hidePicker"> & Partial<Pick<typeof nativeApi, "showCopiedNotice" | "copyHint">> & {
  output: () => OutputToken; pinned: () => boolean; consume: (output: OutputToken) => unknown;
  refresh: () => void; askFormat: () => void; reportHide: (failed: boolean) => void;
};

export function createSessionController(initial = initialSession()) {
  let state = initial;
  let ports: SessionPorts;
  const listeners = new Set<() => void>();
  const dispatch = (event: SessionEvent) => {
    const [next, effects] = reduceSession(state, event);
    if (next !== state) { state = next; for (const notify of listeners) notify(); }
    for (const effect of effects) void run(effect);
  };
  const run = async (effect: SessionEffect) => {
    switch (effect.type) {
      case "copy-hint": {
        let paste_hint = false;
        try { paste_hint = await ports.copyHint?.() ?? false; } catch { /* Learning feedback never undoes a successful copy. */ }
        dispatch({ ...effect, paste_hint });
        break;
      }
      case "copy": {
        try {
          const outcome = await ports.copyFileCitation(effect.ticket, effect.hash);
          dispatch({ type: "copied", id: effect.id, session: effect.session, outcome, output: ports.output(), pinned: ports.pinned() });
        } catch (error) {
          const stale = nativeApi.isSnapshotChanged(error), shared = nativeApi.sharedCitationKey(error);
          dispatch({ type: "copy-failed", id: effect.id, session: effect.session, output: ports.output(), stale,
            message: stale ? pickerText.snapshotChanged : shared ? pickerText.sharedKey(shared) : pickerText.copyError });
        }
        break;
      }
      case "hide":
        try { await ports.hidePicker(); dispatch({ type: "hid", id: effect.id, session: effect.session }); }
        catch { dispatch({ type: "hide-failed", id: effect.id, session: effect.session }); }
        break;
      case "notice": {
        let delivered: CopyReceipt | null = null;
        try { delivered = await ports.showCopiedNotice?.(effect.outcome.text, effect.outcome.delivery ?? null) ?? null; }
        catch { console.error(copiedNoticeText.noticeFailed); }
        dispatch({ type: "delivered", id: effect.id, session: effect.session, delivered });
        break;
      }
      case "consume": ports.consume(effect.output); break;
      case "hide-status": ports.reportHide(effect.failed); break;
      case "refresh": ports.refresh(); break;
      case "ask-format": ports.askFormat(); break;
    }
  };
  return { dispatch, connect: (next: SessionPorts) => { ports = next; }, getSnapshot: () => state,
    subscribe: (notify: () => void) => { listeners.add(notify); return () => { listeners.delete(notify); }; } };
}
