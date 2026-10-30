import { useEffect, useState, type ComponentProps, type ReactNode } from "react";
import type { LibraryStatus, ZoteroLibrary } from "./bindings";
import { appLocale, isolate, libraryMessages, pickerText } from "./messages";
import { Menu, MenuDetail } from "./ui/Menu";
import "./ui/LibraryStatus.css";

/** Done, Look or Stop for the footer dot; words carry the same meaning. */
export function libraryHealth(library: LibraryStatus | null | undefined): "done" | "look" | "stop" | "idle" {
  if (library === null || library?.error || library?.stale) return "stop";
  if (!library?.has_snapshot) return "idle";
  return library.warnings > 0 || library.references === 0 ? "look" : "done";
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
    let timer: number | undefined;
    const visible = () => document.visibilityState !== "hidden" && !document.documentElement.hasAttribute("data-picker-hidden");
    const sync = () => {
      if (timer !== undefined) window.clearInterval(timer);
      timer = undefined;
      if (!visible()) return;
      tick();
      timer = window.setInterval(() => { if (visible()) tick(); }, 30_000);
    };
    const observer = new MutationObserver(sync);
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-picker-hidden"] });
    document.addEventListener("visibilitychange", sync);
    window.addEventListener("focus", sync);
    sync();
    return () => {
      window.clearInterval(timer);
      observer.disconnect();
      document.removeEventListener("visibilitychange", sync);
      window.removeEventListener("focus", sync);
    };
  }, []);
}

/**
 * The footer's library item: a dot, the source and count, and an age only when
 * it matters (just updated, or an older copy kept after a failure). The full
 * detail is the tooltip; the live region holds no clock, so it is not repeated
 * every age tick.
 */
export function LibraryStatusLine({ library, groupName, menu, action, failure }: {
  library: LibraryStatus | null | undefined; groupName: (id: number) => string;
  menu?: Omit<ComponentProps<typeof Menu>, "trigger" | "triggerTitle" | "triggerClassName">;
  action?: ReactNode;
  failure?: string | null;
}) {
  useMinuteTick();
  const now = Date.now();
  const health = libraryHealth(library);
  const [source, summaryState] = libraryMessages.summaryParts(library);
  // The English phrase keeps source, separator, state and age in that order,
  // even on an RTL page or when the source starts in Arabic/Hebrew. The source
  // still chooses its own direction inside the phrase's isolation boundary.
  const separated = source !== null && summaryState.startsWith(" · ");
  const state = separated ? summaryState.slice(" · ".length) : summaryState;
  const age = libraryMessages.age(library, now);
  const lines = libraryMessages.detail(library, now, (status) => zoteroLibraryNames(status, groupName));
  const detail = lines.filter(Boolean).join("\n");
  // A kept copy's age ("checked …") is already in the second line.
  const heard = age && !library?.stale ? `${lines[0]} · ${age}` : lines[0];
  const spoken = age || lines[1] ? [heard, lines[1]].filter(Boolean).join(". ") : null;
  // Recovery and initial opening have their own single announcement in the
  // middle area. Inline operation failures use an alert beside their action.
  const announces = !failure && !(library && !library.has_snapshot && (library.state === "error" || library.state === "loading"));
  const content = <>
        <span className="status-dot" data-health={health} aria-hidden="true" />
        <bdi className="library-words" dir="ltr">
          <span className="library-name" dir="ltr" role="status" aria-label={pickerText.libraryStatus} aria-live={announces ? "polite" : "off"} aria-atomic="true">
            {source !== null && <bdi className="library-source">{source}</bdi>}
            {separated && state && <bdi className="library-separator">{" · "}</bdi>}
            {state && <bdi className="library-state">{state}</bdi>}
          </span>
          {age && <bdi className="library-age" dir="auto" aria-hidden="true"> · {age}</bdi>}
        </bdi>
    </>;
  return (
    <>
      {menu ? <span className="library-status-controls" data-health={health}>
        <span className="library-status-target">
          {/* The live text is a sibling of the button: button descendants can
              become presentational in native accessibility trees. Its stable
              name also avoids announcing a changing button name a second time. */}
          <span className="library-status" data-health={health} title={detail}>{content}</span>
          <Menu {...menu} trigger={null} triggerLabel={pickerText.libraryRegion} triggerClassName="library-status-trigger" triggerTitle={detail} />
        </span>
        {action}
      </span> : <span className="library-status" data-health={health} title={detail}>{content}</span>}
      {/* The visible age and the hover detail, read once by screen readers; not live.
          A <p>, like the state's <bdi>: WebKitGTK drops the text of a plain span
          that sits beside other exposed elements, so Orca would not hear it. */}
      {spoken && <p className="visually-hidden library-detail">{spoken}</p>}
      {failure && <p className="library-action-error" role="alert">{failure}</p>}
    </>
  );
}

/** The library detail at the top of More: a short line, then a quieter one. */
export function LibraryDetail({ lines }: { lines: [string, string | null] }) {
  return <MenuDetail title={lines[0]} detail={lines[1]} />;
}
