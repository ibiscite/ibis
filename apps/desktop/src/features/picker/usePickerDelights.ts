import type * as nativeApi from "../../api";
import { useCallback, useLayoutEffect, useRef, useState } from "react";
import type { DelightView, LibraryStatus } from "../../bindings";
import { type MessageId, useInterfaceMessages } from "../../i18n";
import { keyLabel, keysFor } from "../../keys";
import { isMac } from "../../platform";
import { localMoment } from "../../ui/localMoment";
import { useTimeout } from "../../ui/timers";
import type { EnterAction, RequestedAction } from "./session";

const STATUS: Record<NonNullable<DelightView["status"]>, MessageId> = {
  "paste-tip": "delight-tip", library100: "delight-library100", library1000: "delight-library1000", library10000: "delight-library10000",
};

export function usePickerDelights(api: typeof nativeApi, presentation: number | null, library: LibraryStatus | null | undefined, settingsRevision?: boolean | null) {
  const { text } = useInterfaceMessages();
  const [view, setView] = useState<DelightView | null>(null);
  const dismiss = useCallback(() => setView(null), []);
  // The first screen's practice paste belongs to its opening; typing never retires it, a paste does.
  const [practice, setPractice] = useState<{ presentation: number; pasted: string | null; dismissed?: boolean } | null>(null);
  const request = useRef<{ presentation: number; answer: Promise<EnterAction> } | null>(null);
  // The read that will decide what this opening's Enter does, until it answers:
  // the bar names no action before Enter's own answer is known.
  const [deciding, setDeciding] = useState<object | null>(null);
  const revision = library?.revision ?? null;
  const load = `${library?.state}:${library?.note}`;
  // biome-ignore lint/correctness/useExhaustiveDependencies: a load finishing or clearing its notice can make the same snapshot eligible.
  useLayoutEffect(() => {
    let live = true;
    setView(null);
    if (presentation === null || !api.delightView) {
      request.current = null;
      setDeciding(null);
      return;
    }
    // Only one read per opening can claim the offer. Finish an outstanding
    // read before refreshing so a later read cannot take that answer from Enter.
    const earlier = request.current?.presentation === presentation ? request.current.answer : null;
    const read = async (action: EnterAction): Promise<EnterAction> => {
      if (!live) return action;
      try {
        const next = await api.delightView(localMoment(), presentation, revision, false);
        if (live) setView(next);
        if (!next.practice) return action;
        if (request.current?.presentation === presentation) {
          setPractice((current) => current?.presentation === presentation ? current : { presentation, pasted: null });
        }
        return "practice";
      } catch { return action; }
    };
    const pending = { presentation, answer: earlier ? earlier.then(read) : read("deliver") };
    request.current = pending;
    setDeciding(pending);
    void pending.answer.then((action) => {
      setDeciding((current) => current === pending ? null : current);
      if (request.current === pending && action === "deliver") request.current = null;
    });
    return () => { live = false; };
  }, [api.delightView, presentation, revision, load, settingsRevision]);
  // Retire the animation answer, so remounting the empty state cannot replay it.
  useTimeout(() => setView((current) => current?.peck ? { ...current, peck: false } : current), view?.peck ? 240 : null, view);
  const current = practice && practice.presentation === presentation ? practice : null;
  const actionForEnter = (): RequestedAction => current ? (current.pasted === null ? "practice" : "deliver")
    : request.current?.presentation === presentation ? request.current.answer : "deliver";
  return { practice: current?.dismissed ? null : current, actionForEnter,
    /** Enter's outcome waits on this opening's offer: practice or deliver is not known yet. */
    deciding: deciding !== null && current === null,
    practise: (citation: string) => setPractice((offered) => offered?.presentation === presentation ? { ...offered, pasted: citation } : offered),
    endPractice: () => setPractice((offered) => offered?.pasted ? { ...offered, dismissed: true } : offered), feedback: view?.status ? text(STATUS[view.status], { keys: keyLabel(keysFor("pin"), isMac) }) : null, peck: view?.peck ?? false, passive: view?.status !== "paste-tip", dismiss };
}
