import { useEffect, useMemo, useRef, useState } from "react";
import type { PickerApi } from "../../App";
import type { LibraryStatus } from "../../api";
import { isSnapshotChanged } from "../../api";
import type { SourcePresentation } from "../../bindings";
import { pickerText } from "../../messages";

/** A source query/open owns its item, snapshot and picker presentation. Icons
 * are read once on that identity, only while shown; they never poll. */
export function useSourceOpen(api: Pick<PickerApi, "openLibrarySource" | "librarySource">, library: LibraryStatus | null | undefined,
  visibility: number, item: unknown | null, zoteroOnly: boolean, refresh: () => void) {
  const [visible, setVisible] = useState(false);
  useEffect(() => {
    const sync = () => setVisible(document.visibilityState !== "hidden" && !document.documentElement.hasAttribute("data-picker-hidden"));
    const observer = new MutationObserver(sync);
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-picker-hidden"] });
    document.addEventListener("visibilitychange", sync);
    sync();
    return () => { observer.disconnect(); document.removeEventListener("visibilitychange", sync); };
  }, []);
  const identity = JSON.stringify(item);
  const scope = useMemo(() => ({ library, visibility, identity, zoteroOnly }), [library, visibility, identity, zoteroOnly]);
  const latest = useRef(scope);
  latest.current = scope;
  const request = useRef(0);
  const [resolved, setResolved] = useState<{ scope: typeof scope; source: SourcePresentation } | null>(null);
  const [failure, setFailure] = useState<{ scope: typeof scope; message: string; kind: "zotero" | "file"; changed: boolean } | null>(null);
  const read = api.librarySource;
  const withStatusIcon = Boolean(library?.error || library?.stale || failure?.scope === scope);
  // biome-ignore lint/correctness/useExhaustiveDependencies: scope captures immutable item identity; objects recreated by a render cannot restart an icon read.
  useEffect(() => {
    if (!visible || !read || !library) return;
    let current = true;
    void read(item, library.revision, zoteroOnly, withStatusIcon).then((source) => {
      if (current && latest.current === scope) setResolved({ scope, source });
    // Presentation is optional. Retrying it by refreshing status could loop
    // on a retired reference; the explicit open owns any actionable refusal.
    }).catch(() => {});
    return () => { current = false; };
  }, [read, scope, visible, withStatusIcon]);
  useEffect(() => () => { request.current += 1; }, []);
  const source: SourcePresentation = resolved?.scope === scope ? resolved.source : {
    kind: zoteroOnly || (library?.snapshot_zotero ?? library?.zotero) ? "zotero" : "file",
    name: zoteroOnly ? "Zotero" : library?.snapshot_name ?? library?.name ?? "library", icon: null, status_icon: null,
  };
  const run = (reveal = false, appOnly = false) => {
    if (!api.openLibrarySource) return;
    const current = ++request.current;
    setFailure(null);
    const kind = appOnly ? "zotero" : source.kind;
    void api.openLibrarySource(appOnly ? null : item, library?.revision ?? "", appOnly || zoteroOnly, reveal).catch((error: unknown) => {
      if (latest.current !== scope || request.current !== current) return;
      const changed = isSnapshotChanged(error);
      setFailure({ scope, kind, changed, message: changed ? pickerText.sourceChanged : pickerText.sourceOpenFailed(kind === "zotero") });
      if (changed) refresh();
    });
  };
  return { run, source, failure: failure?.scope === scope ? failure : null };
}
