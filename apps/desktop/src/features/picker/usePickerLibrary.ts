import { useMemo, useRef, useState } from "react";
import type { PickerApi } from "../../App";
import type { useOperations } from "./useOperations";
import { useLibraryStatus } from "./useLibraryStatus";

export function usePickerLibrary(api: PickerApi, report: ReturnType<typeof useOperations>["report"], cancelCopy: () => void) {
  const { chooseLibrary, useZotero, findSources, openFoundLibrary } = api;
  const setupApi = useMemo(() => ({ chooseLibrary, useZotero, useSampleLibrary: api.useSampleLibrary, findSources, openFoundLibrary }),
    [chooseLibrary, useZotero, api.useSampleLibrary, findSources, openFoundLibrary]);
  const { library, libraryVersion, refreshSnapshot } = useLibraryStatus(api, report, cancelCopy);
  const [choosingLibrary, setChoosingLibrary] = useState(false);
  const chooserRequest = useRef<object | null>(null);
  const [welcomeShown, setWelcomeShown] = useState(false);
  const busy = library?.state === "loading" || library?.state === "reloading";
  const blocked = library?.state === "error" && !library.has_snapshot;
  const libraryChoices = choosingLibrary || library?.state === "none" || blocked || library?.state === "loaded" && library.has_snapshot && library.references === 0;
  const showCitations = !libraryChoices;
  return { library, libraryVersion, refreshSnapshot, choosingLibrary, setChoosingLibrary, chooserRequest, welcomeShown, setWelcomeShown, busy, blocked, libraryChoices, showCitations, setupApi };
}
