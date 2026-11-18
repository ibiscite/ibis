import type * as nativeApi from "../../api";
import { useCallback, useEffect, useState } from "react";
import type { DelightView, LibraryStatus } from "../../bindings";
import { type MessageId, useInterfaceMessages } from "../../i18n";
import { keyLabel, keysFor } from "../../keys";
import { isMac } from "../../platform";
import { localMoment } from "../../ui/localMoment";
import { useTimeout } from "../../ui/timers";

const STATUS: Record<NonNullable<DelightView["status"]>, MessageId> = {
  "paste-tip": "delight-tip", library100: "delight-library100", library1000: "delight-library1000", library10000: "delight-library10000",
};

export function usePickerDelights(api: typeof nativeApi, presentation: number | null, library: LibraryStatus | null | undefined, settingsRevision?: boolean | null) {
  const { text } = useInterfaceMessages();
  const [view, setView] = useState<DelightView | null>(null);
  const dismiss = useCallback(() => setView(null), []);
  // The first screen's practice paste belongs to its opening; typing never retires it, a paste does.
  const [practice, setPractice] = useState<{ presentation: number; pasted: string | null } | null>(null);
  const revision = library?.revision ?? null;
  const load = `${library?.state}:${library?.note}`;
  // biome-ignore lint/correctness/useExhaustiveDependencies: a load finishing or clearing its notice can make the same snapshot eligible.
  useEffect(() => {
    let live = true;
    setView(null);
    if (presentation !== null && api.delightView) {
      void api.delightView(localMoment(), presentation, revision, false).then((next) => {
        if (!live) return;
        setView(next);
        if (next.practice) setPractice({ presentation, pasted: null });
      }).catch(() => undefined);
    }
    return () => { live = false; };
  }, [api.delightView, presentation, revision, load, settingsRevision]);
  // Retire the animation answer, so remounting the empty state cannot replay it.
  useTimeout(() => setView((current) => current?.peck ? { ...current, peck: false } : current), view?.peck ? 240 : null, view);
  const current = practice && practice.presentation === presentation ? practice : null;
  return { practice: current, practise: (citation: string) => setPractice((offered) => offered && { ...offered, pasted: citation }),
    endPractice: () => setPractice(null), feedback: view?.status ? text(STATUS[view.status], { keys: keyLabel(keysFor("pin"), isMac) }) : null, peck: view?.peck ?? false, passive: view?.status !== "paste-tip", dismiss };
}
