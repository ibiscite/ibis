import { useCallback, useEffect, useState } from "react";
import type { PickerApi } from "../../App";
import type { DelightView, LibraryStatus } from "../../bindings";
import { type MessageId, useInterfaceMessages } from "../../i18n";
import { localMoment } from "../../ui/localMoment";

const STATUS: Record<NonNullable<DelightView["status"]>, MessageId> = {
  "paste-tip": "delight-tip", library100: "delight-library100", library1000: "delight-library1000", library10000: "delight-library10000",
};

export function usePickerDelights(api: PickerApi, presentation: number | null, library: LibraryStatus | null | undefined, settingsRevision?: boolean | null) {
  const { text } = useInterfaceMessages();
  const [view, setView] = useState<DelightView | null>(null);
  const dismiss = useCallback(() => setView(null), []);
  const revision = library?.revision ?? null;
  const load = `${library?.state}:${library?.note}`;
  // biome-ignore lint/correctness/useExhaustiveDependencies: a load finishing or clearing its notice can make the same snapshot eligible.
  useEffect(() => {
    let live = true;
    let timer: number | undefined;
    setView(null);
    if (presentation !== null && api.delightView) {
      void api.delightView(localMoment(), presentation, revision, false).then((next) => {
        if (!live) return;
        setView(next);
        // Retire the animation answer, so remounting the empty state cannot replay it.
        if (next?.peck) timer = window.setTimeout(() => setView((current) => current === next ? { ...next, peck: false } : current), 240);
      }).catch(() => undefined);
    }
    return () => { live = false; window.clearTimeout(timer); };
  }, [api.delightView, presentation, revision, load, settingsRevision]);
  return { feedback: view?.status ? text(STATUS[view.status], { library: view.library ?? "" }) : null, peck: view?.peck ?? false, passive: view?.status !== "paste-tip", dismiss };
}
