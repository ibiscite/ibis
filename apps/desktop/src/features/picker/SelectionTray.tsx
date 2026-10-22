import { useCallback, useEffect, useLayoutEffect, useRef } from "react";
import type { SearchResult } from "../../api";
import { provisional, type FormattedCitation } from "../../format/formatted";
import { render } from "../../format/markup";
import type { StyleId } from "../../format/styles";
import { pickerText } from "../../messages";

/** Marks the tray with data-more while chips are below its edge, for the fade
 * that says it continues (#746). */
function useMoreBelow<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  const update = useCallback(() => {
    const area = ref.current;
    area?.toggleAttribute("data-more", area.scrollHeight - area.scrollTop - area.clientHeight > 1);
  }, []);
  useLayoutEffect(update);
  useEffect(() => {
    addEventListener("resize", update);
    return () => removeEventListener("resize", update);
  }, [update]);
  return [ref, update] as const;
}

/** The references chosen for one citation, each with its Remove button,
 * after a live region that says how many there are. */
export function SelectionTray({ announce, selected, removeLabel, onRemove }: {
  /** False until the selection first changes, so opening the picker is silent. */
  announce: boolean;
  selected: Map<string, SearchResult>;
  removeLabel: (r: SearchResult) => string;
  onRemove: (r: SearchResult) => void;
}) {
  const [tray, onScroll] = useMoreBelow<HTMLUListElement>();
  return (
    <>
      <p className="visually-hidden" role="status" aria-label={pickerText.citationSelection} aria-live="polite" aria-atomic="true">
        {announce ? pickerText.selectionCount(selected.size) : ""}
      </p>
      {selected.size > 0 && (
        <ul ref={tray} onScroll={onScroll} aria-label={pickerText.selectedReferences}>
          {[...selected.entries()].map(([key, r]) => (
            <li key={key}>
              <code title={r.key}>{r.key}</code>{" "}
              <button type="button" onClick={() => onRemove(r)} aria-label={removeLabel(r)}>
                {pickerText.remove}
              </button>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

/** What a formatted citation will look like, or why it cannot be copied yet. */
export function FormatPreview({ style, shown, focusable }: {
  style: StyleId;
  /** The formatted citation; null while it is being made. */
  shown: FormattedCitation | "failed" | "stale" | null;
  /** A built citation can be scrolled by keyboard. */
  focusable: boolean;
}) {
  return (
    <section aria-label={pickerText.preview} className="format-preview" tabIndex={focusable ? 0 : undefined}>
      {shown === null && <p role="status">{pickerText.previewPending}</p>}
      {shown === "failed" && <p role="alert">{pickerText.previewError}</p>}
      {shown === "stale" && <p role="alert">{pickerText.snapshotChanged}</p>}
      {shown && shown !== "failed" && shown !== "stale" && (
        <>
          <div className="preview-text">{render(shown.markup)}</div>
          {provisional(style) && <p className="provisional">{pickerText.provisional}</p>}
          {shown.warnings.length > 0 && (
            <ul aria-label={pickerText.copyWarnings}>
              {shown.warnings.map((w, i) => <li key={i}>{w}</li>)}
            </ul>
          )}
        </>
      )}
    </section>
  );
}
