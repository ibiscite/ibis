import type * as nativeApi from "../../api";
import { useEffect, useMemo, useRef, useState } from "react";
import type { useOperations } from "./useOperations";
import { useLibraryStatus } from "./useLibraryStatus";

export function usePickerLibrary(api: typeof nativeApi, { report, setFailures }: Pick<ReturnType<typeof useOperations>, "report" | "setFailures">, cancelCopy: () => void) {
  const { chooseLibrary, useZotero, findZotero, findSources, openFoundLibrary } = api;
  const setupApi = useMemo(() => ({ chooseLibrary, useZotero, useSampleLibrary: api.useSampleLibrary, findZotero, findSources, openFoundLibrary }),
    [chooseLibrary, useZotero, api.useSampleLibrary, findZotero, findSources, openFoundLibrary]);
  const { library, settledLibrary, libraryVersion, refreshSnapshot } = useLibraryStatus(api, report, cancelCopy);
  // Setup failures belong to setup; recovery answers an earlier Zotero failure.
  const setupDone = library !== undefined && library?.state !== "none";
  useEffect(() => {
    if (setupDone) setFailures(({ findSources: _found, openFound: _opened, ...rest }) => rest);
  }, [setupDone, setFailures]);
  const healthy = library?.state === "loaded" && !library.error;
  useEffect(() => {
    if (healthy) setFailures(({ zotero: _zotero, ...rest }) => rest);
  }, [healthy, setFailures]);
  const [choosingLibrary, setChoosingLibrary] = useState(false);
  const chooserRequest = useRef<object | null>(null);
  const [welcomeShown, setWelcomeShown] = useState(false);
  const busy = library?.state === "loading" || library?.state === "reloading";
  const blocked = library?.state === "error" && !library.has_snapshot;
  const libraryChoices = choosingLibrary || library?.state === "none" || blocked || library?.state === "loaded" && library.has_snapshot && library.references === 0;
  const showCitations = !libraryChoices;
  return { library, settledLibrary, libraryVersion, refreshSnapshot, choosingLibrary, setChoosingLibrary, chooserRequest, welcomeShown, setWelcomeShown, busy, blocked, libraryChoices, showCitations, setupApi };
}
