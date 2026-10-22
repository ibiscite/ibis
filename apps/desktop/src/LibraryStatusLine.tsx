import { useEffect, useState } from "react";
import type { LibraryStatus, ZoteroLibrary } from "./bindings";
import { appLocale, isolate, libraryMessages, pickerText } from "./messages";

/** A same-source freshness failure whose retained index remains searchable. */
export function hasRetainedFreshnessProblem(library: LibraryStatus | null | undefined): boolean {
  return Boolean(library?.has_snapshot && library.stale && library.error);
}

/** Done, Look or Stop for the footer dot; words carry the same meaning. */
export function libraryHealth(library: LibraryStatus | null | undefined): "done" | "look" | "stop" | "idle" {
  if (library === null || (library?.error && !hasRetainedFreshnessProblem(library))) return "stop";
  if (!library?.has_snapshot) return "idle";
  return hasRetainedFreshnessProblem(library) || library.warnings > 0 || library.references === 0 ? "look" : "done";
}

const lists = new Intl.ListFormat(appLocale, { style: "short", type: "unit" });

/** "My Library, Lab group", naming groups once their names are known. */
export function zoteroLibraryNames(status: LibraryStatus, groupName: (id: number) => string) {
  const names = status.zotero_libraries.map((choice: ZoteroLibrary) =>
    isolate(typeof choice === "object" ? groupName(choice.group) : pickerText.myLibrary));
  return names.length > 0 ? lists.format(names) : isolate(pickerText.myLibrary);
}

/** Re-renders only its user, so a relative time stays right while the picker
 * is open: every 30 seconds while visible, and at once when shown again. */
function useMinuteTick() {
  const [, setTick] = useState(0);
  useEffect(() => {
    const tick = () => setTick((n) => n + 1);
    const timer = window.setInterval(() => { if (document.visibilityState !== "hidden") tick(); }, 30_000);
    document.addEventListener("visibilitychange", tick);
    window.addEventListener("focus", tick);
    return () => {
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", tick);
      window.removeEventListener("focus", tick);
    };
  }, []);
}

/**
 * The footer's library item: a dot, the source and count, and an age only when
 * it matters (just updated, or an older copy kept after a failure). The full
 * detail is the tooltip; the live region holds no clock, so it is not repeated
 * every minute.
 */
export function LibraryStatusLine({ library, groupName }: {
  library: LibraryStatus | null | undefined; groupName: (id: number) => string;
}) {
  useMinuteTick();
  const now = Date.now();
  const health = libraryHealth(library);
  const [source, summaryState] = libraryMessages.summaryParts(library);
  // The separator is its own item, so it stays between the name and the state
  // when a right-to-left page reverses them; inside the state's <bdi> it would
  // be isolated with the English words and end up on their far side.
  const separated = source !== null && summaryState.startsWith(" · ");
  const state = separated ? summaryState.slice(" · ".length) : summaryState;
  const age = libraryMessages.age(library, now);
  const lines = libraryMessages.detail(library, now, (status) => zoteroLibraryNames(status, groupName));
  const detail = lines.filter(Boolean).join("\n");
  // A kept copy's age ("checked …") is already in the second line.
  const heard = age && !library?.stale ? `${lines[0]} · ${age}` : lines[0];
  const spoken = age || lines[1] ? [heard, lines[1]].filter(Boolean).join(". ") : null;
  return (
    <>
      <span className="library-status" data-health={health} title={detail}>
        <span className="status-dot" data-health={health} aria-hidden="true" />
        <span className="library-words">
          <span className="library-name" role="status" aria-label={pickerText.libraryStatus} aria-live="polite" aria-atomic="true">
            {source !== null && <bdi className="library-source">{source}</bdi>}
            {separated && state && <bdi className="library-separator">{" · "}</bdi>}
            {state && <bdi className="library-state">{state}</bdi>}
          </span>
          {age && <span className="library-age" aria-hidden="true"> · {age}</span>}
        </span>
      </span>
      {/* The visible age and the hover detail, read once by screen readers; not live.
          A <p>, like the state's <bdi>: WebKitGTK drops the text of a plain span
          that sits beside other exposed elements, so Orca would not hear it. */}
      {spoken && <p className="visually-hidden library-detail">{spoken}</p>}
    </>
  );
}

/** The library detail at the top of More: a short line, then a quieter one. */
export function LibraryDetail({ lines }: { lines: [string, string | null] }) {
  return (
    <p className="menu-detail" aria-hidden="true">
      <span className="menu-detail-title">{lines[0]}</span>
      {lines[1] && <span>{lines[1]}</span>}
    </p>
  );
}
