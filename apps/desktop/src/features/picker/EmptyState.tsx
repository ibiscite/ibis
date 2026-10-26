import { useEffect, useState, type ReactNode } from "react";
import type { EmptyLibraryChoices } from "../../LibrarySetup";
import type { LibraryStatus } from "../../api";
import { ChoiceIcon } from "../../icons";
import { keysFor } from "../../keys";
import { keyText, libraryMessages, pickerText, setupText } from "../../messages";
import { ChoiceList, type Choice } from "../../ui/ChoiceList";
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

/** A library that could not be opened and has no earlier snapshot to search:
 * say what happened in plain words and offer the fix as choice rows [C12]. */
function Recovery({ library, onFind, onReload, setup }: {
  library: LibraryStatus;
  onFind: () => void;
  onReload: () => void;
  setup: (focus: boolean, empty?: EmptyLibraryChoices) => ReactNode;
}) {
  const [switching, setSwitching] = useState(false);
  // The chosen row is gone, so the list that replaces it takes focus.
  if (switching) return <div className="empty-state">{setup(true)}</div>;
  const name = shortName(library.name) ?? (library.zotero ? setupText.zotero : setupText.theLibrary);
  const reason = library.error ? setupText.sentence(library.error) : undefined;
  const problem = library.problem ?? "other";
  const [title, line] = library.zotero && problem === "zotero_closed" ? [setupText.zoteroClosedTitle, setupText.zoteroClosedLine]
    : library.zotero && problem === "zotero_access_off" ? [setupText.zoteroAccessOffTitle, setupText.zoteroAccessOff]
    : problem === "missing" ? [setupText.missingTitle(name), setupText.missingLine]
    : problem === "unreadable" ? [setupText.unreadableTitle(name), reason]
    : [setupText.otherTitle(name), reason];
  const tryAgain: Choice = { id: "retry", title: setupText.tryAgain, help: setupText.tryAgainHelp, icon: <ChoiceIcon name="retry" /> };
  const another: Choice = { id: "another", title: setupText.another, help: setupText.anotherHelp, icon: <ChoiceIcon name="swap" /> };
  const choices: Choice[] = problem === "missing" && !library.zotero
    ? [{ id: "find", title: setupText.find(name), help: setupText.findFileHelp, icon: <ChoiceIcon name="find" />, suggested: true }, another]
    : [{ ...tryAgain, suggested: true }, another];
  return (
    <div className="empty-state">
      {/* Announced once, as the footer's live region does for other states. */}
      <p className="visually-hidden" role="alert">{line ? `${title}. ${line}` : title}</p>
      <ChoiceList title={title} line={line} choices={choices}
        onChoose={(id) => id === "find" ? onFind() : id === "retry" ? onReload() : setSwitching(true)} />
    </div>
  );
}

/** What the result list says when it has no rows: an empty library, no
 * matches and what to try next, a hint to type, first-run setup, or how to
 * recover a library that could not be opened. */
export function EmptyState({ library, query, narrowed, onChooseLibrary, onClearSearch, onSearchAll, onReload, setup }: {
  library: LibraryStatus | null;
  query: string;
  /** Some Zotero libraries are left out of the search, so searching all may help. */
  narrowed: boolean;
  onChooseLibrary: () => void;
  onClearSearch: () => void;
  onSearchAll: () => void;
  onReload: () => void;
  /** Shown while no library is chosen; focus moves into it when asked. */
  setup: (focus: boolean, empty?: EmptyLibraryChoices) => ReactNode;
}) {
  if (!library) return <div className="empty-state" />;
  const loading = library.state === "loading" || library.state === "reloading";
  if (loading) return <LibraryProgress key={`${library.state}\u0000${library.name}`} library={library} />;
  if (library.state === "error" && !library.has_snapshot) {
    return <Recovery key={`${library.name}\u0000${library.problem}`} library={library} onFind={onChooseLibrary} onReload={onReload} setup={setup} />;
  }
  if (!library.name) return <div className="empty-state">{library.state === "none" ? setup(false) : null}</div>;
  if (library.state === "loaded" && library.has_snapshot && library.references === 0) {
    const fullTitle = libraryMessages.emptyTitle(library.snapshot_name || null);
    const title = libraryMessages.emptyTitle(shortName(library.snapshot_name));
    return <div className="empty-state">{setup(false, {
      title: title === fullTitle ? title : <><span aria-hidden="true">{title}</span><span className="visually-hidden">{fullTitle}</span></>,
      zotero: library.zotero,
    })}</div>;
  }
  const title = query ? pickerText.noMatches : pickerText.typeToFind;
  const description = query ? pickerText.noMatchesHint : pickerText.typeToFindHint;
  return (
    <div className="empty-state empty-template">
      <svg className="empty-icon" aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
        <circle cx="10.5" cy="10.5" r="6.5" /><path d="m16 16 5 5" />
      </svg>
      <p className="empty-title" dir="auto">{title}</p>
      <p className="empty-description">{description}</p>
      {query ? <div className="empty-actions">
        <Button variant="primary" className="empty-primary" onPress={onClearSearch}>{pickerText.clearSearch}</Button>
        {narrowed && <Button variant="quiet" className="empty-alternative" onPress={onSearchAll}>{pickerText.searchAllLibraries}</Button>}
      </div> : null}
      {!query && <div className="empty-keys" aria-hidden="true">
        <KeyHint label={keyText.copy} keys={keysFor("copy")} />
        <KeyHint label={keyText.addToCitation} keys={keysFor("add")} />
      </div>}
    </div>
  );
}
