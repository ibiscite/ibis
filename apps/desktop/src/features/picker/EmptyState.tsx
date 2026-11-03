import { useEffect, useState, type ReactNode } from "react";
import type { EmptyLibraryChoices, SourceChoice } from "../../LibrarySetup";
import type { LibraryStatus } from "../../api";
import { ChoiceIcon, Icon } from "../../icons";
import { keysFor } from "../../keys";
import { keyText, libraryMessages, pickerText, setupText } from "../../messages";
import { ChoiceList, type Choice } from "../../ui/ChoiceList";
import { KeyHint } from "../../ui/KeyCaps";
import { IndeterminateProgress } from "../../ui/Progress";
import { Button } from "../../ui/Button";
import { IbisMark } from "../../ui/IbisMark";
import { zoteroWaits } from "./PickerLibraryStatus";
import { useWhileShown } from "./useWhileShown";
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
  return <div className="empty-opening" role="status" aria-live="polite">
    <IbisMark className="opening-mark" />
    <p className="opening-title" aria-hidden="true" dir="auto">{progress(shortName(name))}</p>
    <p className="opening-line">{setupText.openingLine}</p>
    {/* LibraryStatus reports snapshot counts, never read progress: do not turn
        the retained count into a made-up percentage. */}
    <IndeterminateProgress className="opening-progress update-progress" label={progress(name)}>
      <div className="update-track" aria-hidden="true"><div /></div>
    </IndeterminateProgress>
  </div>;
}

/** How often a screen waiting on Zotero asks it; only while that screen shows. */
export const ZOTERO_CHECK_EVERY = 2_000;

export type ZoteroRecovery = {
  listProfiles?: () => Promise<string[]>;
  chooseProfile?: (name: string) => void;
  checkSharing?: () => Promise<unknown>;
};

/** A library that could not be opened and has no earlier snapshot to search:
 * say what happened in plain words and offer the fix as choice rows [C12]. */
function Recovery({ library, onFind, onReload, setup, source, zotero }: {
  library: LibraryStatus;
  onFind: () => void;
  onReload: () => void;
  setup: (focus: boolean, empty?: EmptyLibraryChoices) => ReactNode;
  source?: SourceChoice;
  zotero?: ZoteroRecovery;
}) {
  const [switching, setSwitching] = useState(false);
  const askingProfile = library.zotero && library.problem === "zotero_profiles";
  // Undefined while listing; null when the list could not be read.
  const [profiles, setProfiles] = useState<string[] | null | undefined>(undefined);
  const listProfiles = askingProfile ? zotero?.listProfiles : undefined;
  useEffect(() => {
    if (!listProfiles) return;
    let current = true;
    void listProfiles().then((found) => { if (current) setProfiles(found); }, () => { if (current) setProfiles(null); });
    return () => { current = false; };
  }, [listProfiles]);
  // While this screen waits on Zotero, and only then, notice the fix by itself.
  const waiting = library.zotero && !switching && zoteroWaits(library.problem);
  useWhileShown(waiting ? zotero?.checkSharing : undefined, ZOTERO_CHECK_EVERY);
  // The chosen row is gone, so the list that replaces it takes focus.
  if (switching) return <div className="empty-state">{setup(true)}</div>;
  const name = shortName(library.name) ?? (library.zotero ? setupText.zotero : setupText.theLibrary);
  const reason = library.error ? setupText.sentence(library.error) : undefined;
  const problem = library.problem ?? "other";
  const [title, line] = library.zotero && problem === "zotero_closed" ? [setupText.zoteroClosedTitle, setupText.zoteroClosedLine]
    : library.zotero && problem === "zotero_access_off" ? [setupText.zoteroAccessOffTitle, `${setupText.zoteroAccessOff} ${setupText.zoteroAccessOffWaiting}`]
    : askingProfile ? [setupText.zoteroProfilesTitle, profiles === null ? setupText.zoteroProfilesFailed : setupText.zoteroProfilesLine]
    : problem === "missing" ? [setupText.missingTitle(name), setupText.missingLine]
    : problem === "unreadable" ? [setupText.unreadableTitle(name), reason]
    : [setupText.otherTitle(name), reason];
  const tryAgain: Choice = { id: "retry", title: setupText.tryAgain, help: setupText.tryAgainHelp, icon: <ChoiceIcon name="retry" /> };
  const another: Choice = { id: "another", title: setupText.another, help: setupText.anotherHelp, icon: <ChoiceIcon name="swap" /> };
  // Finding the file is what ⌘O does here, so it stays one row.
  const finding = problem === "missing" && !library.zotero;
  const profileRows: Choice[] = (profiles ?? []).map((profile, index) => ({ id: `profile-${index}`, title: profile, icon: <ChoiceIcon name="library" /> }));
  const choices: Choice[] = finding
    ? [{ id: "find", title: setupText.find(name), help: setupText.findFileHelp, icon: <ChoiceIcon name="find" />, suggested: true, keys: keysFor("source") }, another]
    : profileRows.length > 0 ? [...profileRows, another]
    // The fix is in Zotero and this screen notices it, so that is the one next step.
    : waiting && source ? [another]
    : [{ ...tryAgain, suggested: true }, another];
  if (!finding && source && source.label !== pickerText.chooseLibraryMenu && !choices.some((choice) => choice.title === source.label)) {
    const open: Choice = { id: "source", title: source.label, icon: source.icon ?? <ChoiceIcon name={library.zotero ? "library" : "file"} />, keys: keysFor("source"), suggested: waiting && profileRows.length === 0 };
    if (open.suggested) choices.unshift(open); else choices.push(open);
  }
  return (
    <div className="empty-state">
      {/* Announced once, as the footer's live region does for other states. */}
      <p className="visually-hidden" role="alert">{line ? `${title}. ${line}` : title}</p>
      <ChoiceList title={title} line={line} choices={choices}
        onChoose={(id) => {
          const profile = id.startsWith("profile-") ? profiles?.[Number(id.slice("profile-".length))] : undefined;
          if (profile !== undefined) zotero?.chooseProfile?.(profile);
          else if (id === "source") source?.open();
          else if (id === "find") onFind();
          else if (id === "retry") onReload();
          else setSwitching(true);
        }} />
    </div>
  );
}

/** What the result list says when it has no rows: an empty library, no
 * matches and what to try next, a hint to type, first-run setup, or how to
 * recover a library that could not be opened. */
export function EmptyState({ library, query, narrowed, onChooseLibrary, onClearSearch, onSearchAll, onReload, setup, source, peck, zotero }: {
  peck?: boolean;
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
  source?: SourceChoice;
  zotero?: ZoteroRecovery;
}) {
  if (!library) return <div className="empty-state" />;
  const loading = library.state === "loading" || library.state === "reloading";
  if (loading) return library.has_snapshot && library.references > 0 ? null
    : <LibraryProgress key={`${library.state}\u0000${library.name}`} library={library} />;
  if (library.state === "error" && !library.has_snapshot) {
    return <Recovery key={`${library.name}\u0000${library.problem}`} library={library} onFind={onChooseLibrary} onReload={onReload} setup={setup} source={source} zotero={zotero} />;
  }
  if (!library.name) return <div className="empty-state">{library.state === "none" ? setup(false) : null}</div>;
  if (library.state === "loaded" && library.has_snapshot && library.references === 0) {
    const fullTitle = libraryMessages.emptyTitle(library.snapshot_name || null);
    const title = libraryMessages.emptyTitle(shortName(library.snapshot_name));
    return <div className="empty-state">{setup(false, {
      title: title === fullTitle ? title : <><span aria-hidden="true">{title}</span><span className="visually-hidden">{fullTitle}</span></>,
      zotero: library.zotero,
      source,
    })}</div>;
  }
  const title = query ? pickerText.noMatches : pickerText.typeToFind;
  const description = query ? pickerText.noMatchesHint : library.sample ? setupText.sampleSearchHelp : pickerText.typeToFindHint;
  return (
    <div className="empty-state empty-template">
      {peck && !query ? <IbisMark className="opened-mark" size="welcome" motion="peck" /> : <Icon name="find" className="empty-icon" />}
      <p className="empty-title" dir="auto">{title}</p>
      <p className="empty-description">{description}</p>
      {query ? <div className="empty-actions">
        <Button variant="primary" className="empty-primary" onPress={onClearSearch}>{pickerText.clearSearch}</Button>
        {narrowed && <Button variant="secondary" className="empty-alternative" onPress={onSearchAll}>{pickerText.searchAllLibraries}</Button>}
      </div> : null}
      {!query && <div className="empty-keys" aria-hidden="true">
        <KeyHint label={keyText.copy} keys={keysFor("copy")} />
        <KeyHint label={keyText.addToCitation} keys={keysFor("add")} />
      </div>}
    </div>
  );
}
