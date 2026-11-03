import { useEffect, useRef, type RefObject } from "react";
import type { LibraryStatus } from "../../api";
import type { SessionEvent } from "./session";
import type { useOperations } from "./useOperations";

export function useLibrarySetup(library: LibraryStatus | null | undefined, setFailures: ReturnType<typeof useOperations>["setFailures"],
  send: (event: SessionEvent) => void, input: RefObject<HTMLInputElement | null>) {
  // Setup failures belong to the setup screen; they go when a library is chosen.
  const setupDone = library !== undefined && library?.state !== "none";
  useEffect(() => {
    if (setupDone) setFailures(({ findSources: _found, openFound: _opened, ...rest }) => rest);
  }, [setupDone, setFailures]);
  // A library that recovered by itself answers any earlier failure to reach it.
  const healthy = library?.state === "loaded" && !library.error;
  useEffect(() => {
    if (healthy) setFailures(({ zotero: _zotero, ...rest }) => rest);
  }, [healthy, setFailures]);
  const sample = Boolean(library?.sample);
  const previousSample = useRef(sample);
  // biome-ignore lint/correctness/useExhaustiveDependencies: only a sample transition resets the citation; Search's ref is stable.
  useEffect(() => {
    if (previousSample.current === sample) return;
    previousSample.current = sample;
    send({ type: "sample" });
    input.current?.focus();
  }, [sample, send]);
}
