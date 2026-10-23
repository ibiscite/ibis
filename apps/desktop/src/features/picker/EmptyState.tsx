import { useEffect, useState, type ReactNode } from "react";
import type { LibraryStatus } from "../../api";
import { keysFor } from "../../keys";
import { keyText, libraryMessages, pickerText } from "../../messages";
import { KeyHint } from "../../ui/KeyCaps";
import { IndeterminateProgress } from "../../ui/Progress";
import { Button } from "../../ui/Button";
import "./EmptyState.css";

// The empty-state contract keeps fast library opens free of transient content.
const LOADING_DELAY_MS = 400;

/** Keep the start and extension without letting an unusually long name
 * push the recovery actions down the page. Older webviews keep the full
 * name rather than guessing where a grapheme can be split. */
function shortName(name: string | null): string | null {
  if (!name) return null;
  if (typeof Intl.Segmenter !== "function") return name;
  const parts = Array.from(new Intl.Segmenter().segment(name), ({ segment }) => segment);
  return parts.length > 40 ? `${parts.slice(0, 20).join("")}…${parts.slice(-19).join("")}` : name;
}

function LibraryProgress({ library }: { library: LibraryStatus }) {
  const [visible, setVisible] = useState(false);
  useEffect(() => {
    const timer = window.setTimeout(() => setVisible(true), LOADING_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, []);
  if (!visible) return null;
  const progress = library.state === "reloading" ? libraryMessages.reloading : libraryMessages.opening;
  const name = library.name || null;
  return <div role="status" aria-live="polite"><IndeterminateProgress className="empty-loading" label={progress(name)}>
    <svg className="empty-spinner" aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
      <path d="M12 3a9 9 0 1 1-9 9" />
    </svg>
    <span aria-hidden="true" dir="auto">{progress(shortName(name))}</span>
  </IndeterminateProgress></div>;
}

/** What the result list says when it has no rows: an empty library, no
 * matches and what to try next, a hint to type, or first-run setup. */
export function EmptyState({ library, query, narrowed, onChooseLibrary, onClearSearch, onSearchAll, onReload, setup }: {
  library: LibraryStatus | null;
  query: string;
  /** Some Zotero libraries are left out of the search, so searching all may help. */
  narrowed: boolean;
  onChooseLibrary: () => void;
  onClearSearch: () => void;
  onSearchAll: () => void;
  onReload: () => void;
  /** Shown while no library is chosen. */
  setup: ReactNode;
}) {
  if (!library) return <div className="empty-state" />;
  const loading = library.state === "loading" || library.state === "reloading";
  if (loading) return <LibraryProgress key={`${library.state}\u0000${library.name}`} library={library} />;
  if (!library.name) return <div className="empty-state">{library.state === "none" ? setup : null}</div>;
  const empty = library.has_snapshot && library.references === 0;
  const fullTitle = libraryMessages.emptyTitle(library.snapshot_name || null);
  const title = empty ? libraryMessages.emptyTitle(shortName(library.snapshot_name))
    : query ? pickerText.noMatches : pickerText.typeToFind;
  const description = empty ? libraryMessages.emptyHint(library.zotero)
    : query ? pickerText.noMatchesHint : pickerText.typeToFindHint;
  return (
    <div className="empty-state empty-template">
      <svg className="empty-icon" aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
        {empty ? <path d="M5 3h9l5 5v13H5zM14 3v5h5M8 12h8M8 16h6" />
          : <><circle cx="10.5" cy="10.5" r="6.5" /><path d="m16 16 5 5" /></>}
      </svg>
      <p className="empty-title" dir="auto">{empty && title !== fullTitle ? <>
        <span aria-hidden="true">{title}</span><span className="visually-hidden">{fullTitle}</span>
      </> : title}</p>
      <p className="empty-description">{description}</p>
      {empty ? <div className="empty-actions">
        {library.zotero && <Button variant="primary" className="empty-primary" onPress={onReload}>{pickerText.reload}</Button>}
        <Button variant={library.zotero ? "quiet" : "primary"} className={library.zotero ? "empty-alternative" : "empty-primary"} onPress={onChooseLibrary}>{pickerText.chooseLibrary}</Button>
      </div> : query ? <div className="empty-actions">
        <Button variant="primary" className="empty-primary" onPress={onClearSearch}>{pickerText.clearSearch}</Button>
        {narrowed && <Button variant="quiet" className="empty-alternative" onPress={onSearchAll}>{pickerText.searchAllLibraries}</Button>}
      </div> : null}
      {!empty && !query && <div className="empty-keys" aria-hidden="true">
        <KeyHint label={keyText.copy} keys={keysFor("copy")} />
        <KeyHint label={keyText.addToCitation} keys={keysFor("add")} />
      </div>}
    </div>
  );
}
