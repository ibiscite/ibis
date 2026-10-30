import { useEffect, useMemo, useRef, useState } from "react";
import type { PickerApi } from "../../App";
import type { LibraryStatus } from "../../api";
import { pickerText } from "../../messages";

/** An open completion owns only its source and presentation. Late failures
 * cannot appear in another library or after the picker is reopened. */
export function useSourceOpen(open: PickerApi["openLibrarySource"], library: LibraryStatus | null | undefined, visibility: number) {
  const scope = useMemo(() => ({ library, visibility }), [library, visibility]);
  const latest = useRef(scope);
  latest.current = scope;
  const request = useRef(0);
  const [failure, setFailure] = useState<{ scope: typeof scope; message: string } | null>(null);
  useEffect(() => () => { request.current += 1; }, []);
  const run = (item: unknown | null, zoteroOnly = false) => {
    if (!open || !library) return;
    const current = ++request.current;
    setFailure(null);
    void open(item, library.revision, zoteroOnly).catch(() => {
      if (latest.current !== scope || request.current !== current) return;
      const zotero = zoteroOnly || (library.has_snapshot ? library.snapshot_zotero : library.zotero);
      setFailure({ scope, message: pickerText.sourceOpenFailed(zotero) });
    });
  };
  return { run, failure: failure?.scope === scope ? failure.message : null };
}
