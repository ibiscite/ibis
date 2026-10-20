import type React from "react";
import * as nativeApi from "./api";
import { copyFormatNames, densityNames, libraryMessages, operationFailures as FAILURE_MESSAGES, pickerText } from "./messages";
import { useEffect, useRef, useState } from "react";
import { formatCitation, FORMATTED_STYLES, provisional, type FormattedCitation } from "./format/formatted";
import { STYLE_NAMES, type StyleId } from "./format/styles";
import { render } from "./format/markup";
import {
  COPY_FORMATS,
  type Copied,
  type CopyFormat,
  type Density,
  type LibraryStatus,
  type PickerPreferences,
  type RecordId,
  type SearchResult,
  type ShortcutStatus,
  type ZoteroGroup,
  type ZoteroLibrary,
} from "./api";

/** True while an input method is composing text; Enter then confirms the text.
 * Some WebKit versions report the confirming Enter with key code 229 only. */
function composing(e: React.KeyboardEvent): boolean {
  return e.nativeEvent.isComposing || e.keyCode === 229;
}

/** Selection is keyed by record identity, never by list position or citation key. */
function idKey(id: RecordId): string {
  return `${id.source}\u0000${id.native_key}`;
}

type Operation = keyof typeof FAILURE_MESSAGES;

const DENSITIES: Density[] = ["compact", "comfortable", "spacious"];

// A search comes back if the picker returns within a minute, as in Raycast.
const KEEP_SEARCH_MS = 60_000;

// Shortcut labels follow the platform: ⌘ on macOS, Ctrl elsewhere.
const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);
const modifier = isMac ? "⌘" : "Ctrl+";
// Ctrl+K is "delete to end of line" in macOS text fields, so there only ⌘K opens More.
const commandKey = (e: React.KeyboardEvent) => (isMac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey);

// Fields a row already shows; a match anywhere else is named under the row.
const VISIBLE_FIELDS = new Set(["title", "author", "citekey", "year"]);

/** The strongest match the row does not show, if every match is hidden. */
function hiddenMatch(matched: string[]): string | null {
  if (matched.length === 0 || matched.some((field) => VISIBLE_FIELDS.has(field))) return null;
  return matched[0] ?? null;
}

/** Done, Look or Stop for the footer dot; words carry the same meaning. */
function libraryHealth(library: LibraryStatus | null | undefined): "done" | "look" | "stop" | "idle" {
  if (library === null || library?.error) return "stop";
  if (!library?.has_snapshot) return "idle";
  return library.warnings > 0 || library.references === 0 ? "look" : "done";
}

/** A small menu: arrow keys move, Escape or a click outside closes it. */
function MoreMenu({ children, onClose }: { children: React.ReactNode; onClose: (refocus: boolean) => void }) {
  const menu = useRef<HTMLDivElement>(null);
  useEffect(() => {
    menu.current?.querySelector<HTMLElement>("[data-menu-item]")?.focus();
    const outside = (e: PointerEvent) => {
      const target = e.target as Node;
      if (!menu.current?.contains(target) && !menu.current?.parentElement?.contains(target)) onClose(false);
    };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, []);
  return (
    <div
      id="more-menu"
      role="menu"
      aria-label={pickerText.more}
      ref={menu}
      onKeyDown={(e) => {
        const items = [...(menu.current?.querySelectorAll<HTMLElement>("[data-menu-item]") ?? [])];
        const at = items.indexOf(document.activeElement as HTMLElement);
        const move = { ArrowDown: 1, ArrowUp: -1 }[e.key];
        if (move !== undefined) {
          e.preventDefault();
          items[(at + move + items.length) % items.length]?.focus();
        } else if (e.key === "Home" || e.key === "End") {
          e.preventDefault();
          items[e.key === "Home" ? 0 : items.length - 1]?.focus();
        } else if (e.key === "Tab") {
          // Leaving the menu returns to the search box, so keys keep working.
          e.preventDefault();
          onClose(false);
          document.getElementById("search")?.focus();
        }
      }}
    >
      {children}
    </div>
  );
}

function MenuItem({ label, title, keys, checked, disabled, onSelect }: {
  label: string; title?: string; keys?: string; checked?: boolean; disabled?: boolean; onSelect: () => void;
}) {
  // A checkable item keeps the menuitem role for focus handling; aria-checked
  // is announced on menuitemcheckbox only, so the role changes with it. A
  // disabled item stays focusable so arrow keys still reach it.
  return (
    <button type="button" role={checked === undefined ? "menuitem" : "menuitemcheckbox"} aria-checked={checked}
      aria-disabled={disabled || undefined} data-menu-item tabIndex={-1} title={title}
      onClick={() => { if (!disabled) onSelect(); }}>
      <span>{label}</span>
      {keys && <kbd aria-hidden="true">{keys}</kbd>}
    </button>
  );
}

// A stable provider lets the development gallery use synthetic data without IPC.
// Formatted citations are offered only when the provider can supply them.
export type PickerApi = Pick<typeof nativeApi,
  "chooseLibrary" | "useZotero" | "libraryStatus" | "reloadLibrary" | "copyCitation" | "copyFormat" | "pickerPreferences" | "setPickerPreferences" |
  "hidePicker" | "setPinned" | "onLibraryChanged" | "onPickerShown" | "onPickerHidden" | "quit" | "search" | "shortcutStatus"> &
  Partial<Pick<typeof nativeApi, "cslItems" | "copyFormatted" | "useZoteroLibraries" | "zoteroGroups">>;

/** A citation-key syntax, or a citation style written as `style:<id>`. */
type Choice = CopyFormat | `style:${StyleId}`;

/** The one word the closed format control shows. */
function choiceName(choice: Choice): string {
  const style = styleOf(choice);
  return style ? STYLE_NAMES[style] : copyFormatNames[choice as CopyFormat];
}

function sameLibrary(a: ZoteroLibrary, b: ZoteroLibrary) {
  return a === b || (typeof a === "object" && typeof b === "object" && a.group === b.group);
}

function styleOf(choice: Choice): StyleId | null {
  return choice.startsWith("style:") ? (choice.slice("style:".length) as StyleId) : null;
}

export type PickerPreview = { query?: string; copied?: Copied };

// The picker: search, choose one or more references, copy them as a citation.
export function App({ api = nativeApi, preview }: { api?: PickerApi; preview?: PickerPreview } = {}) {
  const { chooseLibrary, useZotero, libraryStatus, reloadLibrary, copyCitation, copyFormat,
    hidePicker, setPinned, onLibraryChanged, onPickerShown, onPickerHidden, quit, search, shortcutStatus, cslItems, copyFormatted,
    pickerPreferences, setPickerPreferences, useZoteroLibraries, zoteroGroups } = api;
  const [query, setQuery] = useState(preview?.query ?? "");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [announcement, setAnnouncement] = useState("");
  // The query the shown results belong to; differs while a search is pending.
  const [resultsQuery, setResultsQuery] = useState<string | null>(null);
  const [active, setActive] = useState(0);
  const [selected, setSelected] = useState<Map<string, SearchResult>>(new Map());
  const [selectionChanged, setSelectionChanged] = useState(false);
  const [format, setFormat] = useState<Choice>("pandoc");
  const style = cslItems && copyFormatted ? styleOf(format) : null;
  // The formatted preview, tagged with the style, choice and library it was
  // made for, so Enter never copies a preview of an earlier choice.
  const [formatted, setFormatted] = useState<{ key: string; result: FormattedCitation | "failed" } | null>(null);
  const [copied, setCopied] = useState<Copied | null>(preview?.copied ?? null);
  const [error, setError] = useState<string | null>(null);
  const [copyError, setCopyError] = useState<string | null>(null);
  const [shortcut, setShortcut] = useState<ShortcutStatus | null>(null);
  // Changes whenever a library opens, so the current query is searched again.
  const [libraryVersion, setLibraryVersion] = useState(0);
  // Undefined while loading; null means the status request itself failed.
  const [library, setLibrary] = useState<LibraryStatus | null>();
  const input = useRef<HTMLInputElement>(null);
  const moreButton = useRef<HTMLButtonElement>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const [pinned, setPinnedState] = useState(false);
  // When the picker last went away; the search waits a minute, in memory only.
  const hiddenAt = useRef<number | null>(null);
  const [failures, setFailures] = useState<Partial<Record<Operation, boolean>>>({});
  const report = (operation: Operation, failed = true) => {
    setFailures((previous) => ({ ...previous, [operation]: failed }));
  };
  const perform = async (operation: Operation, action: () => Promise<unknown>) => {
    try { await action(); report(operation, false); }
    catch { report(operation); }
  };
  const hide = () => void perform("hide", hidePicker);
  const clearSearch = () => {
    setQuery("");
    setSelected(new Map());
    setActive(0);
  };
  // Saved preferences, once read; choices made before that wait in `pending`
  // so a slow read never overwrites them and a failed read never resets the
  // file to defaults.
  const [density, setDensity] = useState<Density>("comfortable");
  const [largerText, setLargerText] = useState(false);
  const savedPreferences = useRef<PickerPreferences | null>(null);
  const pending = useRef<Partial<PickerPreferences>>({});
  const savePreferences = (change: Partial<PickerPreferences>) => {
    pending.current = { ...pending.current, ...change };
    if (!savedPreferences.current) return;
    const next = { ...savedPreferences.current, ...pending.current } as PickerPreferences;
    pending.current = {};
    savedPreferences.current = next;
    void perform("preferences", () => setPickerPreferences(next));
  };
  const chooseFormat = (choice: Choice) => {
    setFormat(choice);
    savePreferences({ citation_style: styleOf(choice) });
  };
  const changeDensity = () => {
    const next = DENSITIES[(DENSITIES.indexOf(density) + 1) % DENSITIES.length] ?? "comfortable";
    setDensity(next);
    savePreferences({ density: next });
  };
  const toggleLargerText = () => {
    setLargerText(!largerText);
    savePreferences({ larger_text: !largerText });
  };
  useEffect(() => {
    document.documentElement.dataset.density = density;
    document.documentElement.dataset.textSize = largerText ? "larger" : "normal";
    return () => {
      delete document.documentElement.dataset.density;
      delete document.documentElement.dataset.textSize;
    };
  }, [density, largerText]);

  const togglePin = () => {
    const on = !pinned;
    setPinnedState(on);
    // Show the pin as it really is if the window could not change.
    void perform("pin", () => setPinned(on).catch((error: unknown) => { setPinnedState(!on); throw error; }));
  };

  // When the picker is shown, put the cursor in the search box with the
  // previous query selected, so typing replaces it.
  useEffect(() => {
    let acceptInitialStatus = true;
    let mounted = true;
    const subscriptionFailure = (operation: Operation) => {
      if (mounted) report(operation);
      else console.error(FAILURE_MESSAGES[operation]);
      return () => undefined;
    };
    const focusSearch = () => {
      if (hiddenAt.current !== null && performance.now() - hiddenAt.current > KEEP_SEARCH_MS) clearSearch();
      hiddenAt.current = null;
      input.current?.focus();
      input.current?.select();
    };
    // Every hide is reported, including the native one when focus moves away;
    // a monotonic clock keeps clock changes from clearing or keeping a search.
    const unlistenHidden = onPickerHidden(() => { hiddenAt.current = performance.now(); })
      .catch(() => subscriptionFailure("pickerEvents"));
    const unlisten = onPickerShown(focusSearch).catch(() => subscriptionFailure("pickerEvents"));
    const unlistenLibrary = onLibraryChanged((status) => {
      acceptInitialStatus = false;
      setLibrary(status);
      report("status", false);
      setLibraryVersion((v) => v + 1);
    }).catch(() => subscriptionFailure("libraryEvents"));
    libraryStatus()
      .then((status) => { if (acceptInitialStatus) setLibrary(status); })
      .catch(() => { if (acceptInitialStatus) { setLibrary(null); report("status"); } });
    shortcutStatus()
      .then(setShortcut)
      .catch(() => { setShortcut(null); report("shortcut"); });
    // The saved style wins over the saved key format; a choice made while
    // these load wins over both.
    void Promise.allSettled([copyFormat(), pickerPreferences()]).then(([key, saved]) => {
      if (!mounted) return;
      if (key.status === "rejected") report("format");
      if (saved.status === "rejected") {
        // Without saved preferences the saved key format still applies.
        report("preferences");
        if (pending.current.citation_style === undefined && key.status === "fulfilled") setFormat(key.value);
        return;
      }
      const loaded = { ...saved.value, ...pending.current } as PickerPreferences;
      savedPreferences.current = saved.value;
      setDensity(loaded.density);
      setLargerText(loaded.larger_text);
      const chosen = pending.current.citation_style !== undefined;
      if (Object.keys(pending.current).length > 0) savePreferences({});
      if (chosen) return;
      const style = saved.value.citation_style;
      if (style && cslItems && copyFormatted) setFormat(`style:${style}`);
      else if (key.status === "fulfilled") setFormat(key.value);
    });
    return () => {
      acceptInitialStatus = false;
      mounted = false;
      void unlistenHidden.then((stop) => stop()).catch(() => console.error("Picker event cleanup failed."));
      void unlisten.then((stop) => stop()).catch(() => console.error("Picker event cleanup failed."));
      void unlistenLibrary.then((stop) => stop()).catch(() => console.error("Library event cleanup failed."));
    };
  }, []);

  useEffect(() => {
    setAnnouncement("");
    // Typing before choosing a library is normal; preserve the query and
    // chooser. Actual library/status failures still report search errors.
    if (library === undefined || library?.state === "none" || library?.state === "loading" || (library?.state === "error" && !library.has_snapshot)) {
      setResults([]);
      setResultsQuery(query);
      setActive(0);
      setError(null);
      return;
    }
    let current = true;
    let announcementTimer: number | undefined;
    search(query)
      .then((found) => {
        if (current) {
          setResults(found);
          setResultsQuery(query);
          setActive(0);
          setError(null);
          announcementTimer = window.setTimeout(() => {
            setAnnouncement(pickerText.resultCount(found.length));
          }, 300);
        }
      })
      .catch(() => {
        // Results of an earlier query must not sit beside this query's error.
        if (current) {
          setResults([]);
          setResultsQuery(query);
          setError(pickerText.searchError);
        }
      });
    return () => {
      current = false;
      window.clearTimeout(announcementTimer);
    };
  }, [query, library, libraryVersion]);

  // Keep the highlighted result visible while moving with the arrow keys.
  useEffect(() => {
    document.getElementById(`result-${active}`)?.scrollIntoView?.({ block: "nearest" });
  }, [active]);

  // Success is brief; warnings stay available until explicitly dismissed.
  useEffect(() => {
    if (preview || !copied || copied.warnings.length > 0) return;
    const timer = window.setTimeout(() => setCopied(null), 2400);
    return () => window.clearTimeout(timer);
  }, [copied, preview]);

  const toggle = (result: SearchResult) => {
    setSelectionChanged(true);
    setSelected((previous) => {
      const next = new Map(previous);
      if (!next.delete(idKey(result.id))) next.set(idKey(result.id), result);
      return next;
    });
  };

  // The selection, or the highlighted result when nothing is selected.
  // Never a highlighted result that belongs to an earlier query.
  const current = resultsQuery === query ? results : [];
  const chosen = selected.size > 0 ? [...selected.values()] : current.slice(active, active + 1);
  const chosenKey = chosen.map((r) => idKey(r.id)).join("\u0001");
  const previewKey = `${style}\u0002${libraryVersion}\u0002${chosenKey}`;
  // What Enter would copy now; null while it is being formatted.
  const shown = formatted?.key === previewKey ? formatted.result : null;

  // Format the preview again whenever the choice or the style changes.
  useEffect(() => {
    if (!style || !cslItems || chosen.length === 0) return;
    let live = true;
    const key = previewKey;
    cslItems(chosen.map((r) => r.id))
      .then((csl) => formatCitation(csl, style))
      .then((result) => { if (live) setFormatted({ key, result }); })
      .catch(() => { if (live) setFormatted({ key, result: "failed" }); });
    return () => { live = false; };
  }, [previewKey]);

  // Copy the chosen references in the chosen format.
  const copy = async () => {
    if (chosen.length === 0) return;
    setCopied(null);
    setCopyError(null);
    let outcome: Copied;
    try {
      if (style && copyFormatted) {
        // Copy only what the preview shows, never a result still being made.
        if (shown === null || shown === "failed") {
          setCopyError(shown === null ? pickerText.previewNotReady : pickerText.previewError);
          return;
        }
        await copyFormatted(shown.text, shown.html);
        outcome = { text: shown.text, warnings: shown.warnings };
      } else {
        outcome = await copyCitation(
          chosen.map((r) => r.id),
          format as CopyFormat,
        );
      }
    } catch {
      setCopyError(pickerText.copyError);
      return;
    }
    setCopied(outcome);
    setSelected(new Map());
    // Get out of the way so the user can paste, unless there is something to
    // read or the picker is pinned beside the document.
    if (outcome.warnings.length === 0 && !pinned) hide();
  };

  const busy = library?.state === "loading" || library?.state === "reloading";
  const loadingMessage = library?.state === "reloading"
    ? libraryMessages.reloading(library.name)
    : libraryMessages.opening(library?.name ?? null);

  // The search box is the place to type; the shortcut works from anywhere in the picker.
  const isMenuShortcut = (e: React.KeyboardEvent) =>
    e.key.toLowerCase() === "k" && commandKey(e) && !e.altKey && !e.shiftKey;
  // Zotero libraries: the ones chosen, and the groups Zotero offers, listed
  // when the menu opens or when results come from several libraries.
  const [groups, setGroups] = useState<ZoteroGroup[] | null>(null);
  // A choice sent but not yet reported back, so a second toggle builds on it.
  const [pendingLibraries, setPendingLibraries] = useState<ZoteroLibrary[] | null>(null);
  const reportedLibraries = library?.zotero_libraries ?? [];
  const sameChoice = (a: ZoteroLibrary[], b: ZoteroLibrary[]) =>
    a.length === b.length && a.every((choice, i) => sameLibrary(choice, b[i]!));
  useEffect(() => {
    if (pendingLibraries && sameChoice(pendingLibraries, reportedLibraries)) setPendingLibraries(null);
  }, [library]);
  const chosenLibraries = pendingLibraries ?? reportedLibraries;
  const severalLibraries = chosenLibraries.length > 1;
  const wantGroups = Boolean(zoteroGroups && library?.zotero && (menuOpen || severalLibraries));
  useEffect(() => {
    if (!wantGroups || !zoteroGroups) return;
    let current = true;
    zoteroGroups()
      .then((list) => { if (current) { setGroups(list); report("zoteroGroups", false); } })
      .catch(() => { if (current) report("zoteroGroups"); });
    return () => { current = false; };
  }, [wantGroups, libraryVersion]);
  const isChosen = (choice: ZoteroLibrary) => chosenLibraries.some((chosen) => sameLibrary(chosen, choice));
  const toggleLibrary = (choice: ZoteroLibrary) => {
    if (!useZoteroLibraries) return;
    const next = isChosen(choice)
      ? chosenLibraries.filter((chosen) => !sameLibrary(chosen, choice))
      : [...chosenLibraries, choice];
    if (next.length === 0) return;
    setPendingLibraries(next);
    void perform("zotero", () => useZoteroLibraries(next).catch((error: unknown) => {
      setPendingLibraries(null);
      throw error;
    }));
  };
  const groupName = (id: number) => groups?.find((group) => group.id === id)?.name ?? pickerText.groupLibrary(id);
  // Every group Zotero lists, plus chosen groups it no longer lists, so they can be turned off.
  const offeredGroups = [...(groups?.map((group) => group.id) ?? []),
    ...chosenLibraries.flatMap((chosen) => (typeof chosen === "object" ? [chosen.group] : []))]
    .filter((id, i, all) => all.indexOf(id) === i);
  const libraryOf = (source: string) =>
    source === "zotero:user" ? pickerText.myLibrary
      : source.startsWith("zotero:group:") ? groupName(Number(source.slice("zotero:group:".length))) : null;
  const closeMenu = (refocus: boolean) => {
    setMenuOpen(false);
    if (refocus) moreButton.current?.focus();
  };
  const runFromMenu = (operation: Operation, action: () => Promise<unknown>) => {
    closeMenu(false);
    input.current?.focus();
    void perform(operation, action);
  };
  const health = libraryHealth(library);

  return (
    <main
      onKeyDown={(e) => {
        if (composing(e)) return;
        if (e.key === "Escape") {
          e.preventDefault();
          // Escape steps back one layer: the menu, then the search, then the
          // picker. Chosen references stay; hiding keeps them for a minute.
          if (menuOpen) closeMenu(true);
          else if (query) {
            setQuery("");
            input.current?.focus();
          } else hide();
        } else if (isMenuShortcut(e)) {
          e.preventDefault();
          if (menuOpen) closeMenu(true);
          else setMenuOpen(true);
        } else if (e.key === "q" && (e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey) {
          e.preventDefault();
          void perform("quit", quit);
        }
      }}
    >
      {shortcut?.error && (
        <p role="status">
          {pickerText.shortcutError(shortcut.registered, shortcut.shortcut, shortcut.error)}
        </p>
      )}
      <header className="search-bar" data-tauri-drag-region title={pickerText.dragSearchBar}>
        <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor">
          <circle cx="10.5" cy="10.5" r="6.5" />
          <path d="m16 16 5 5" />
        </svg>
        <label className="visually-hidden" htmlFor="search">
          {pickerText.searchLabel}
        </label>
        <input
          id="search"
          type="search"
          placeholder={pickerText.searchPlaceholder}
          role="combobox"
          aria-expanded={results.length > 0}
          aria-controls="results"
          aria-activedescendant={results[active] ? `result-${active}` : undefined}
          ref={input}
          autoFocus
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
          }}
          onKeyDown={(e) => {
            if (composing(e)) return;
            if (e.key === "ArrowDown" || e.key === "ArrowUp") {
              e.preventDefault();
              const step = e.key === "ArrowDown" ? 1 : -1;
              setActive((i) => Math.min(Math.max(i + step, 0), Math.max(results.length - 1, 0)));
            } else if (e.key === "Enter" && e.shiftKey) {
              e.preventDefault();
              const result = results[active];
              if (result) toggle(result);
            } else if (e.key === "Enter") {
              e.preventDefault();
              void copy();
            }
          }}
        />
        {/* The closed control shows one word; the list and screen readers get the full name. */}
        <span className="format-picker" title={pickerText.copyAs}>
          <span aria-hidden="true" className="format-name">{choiceName(format)}</span>
          <label className="visually-hidden" htmlFor="format">
            {pickerText.copyAs}
          </label>
          <select id="format" value={format} onChange={(e) => chooseFormat(e.target.value as Choice)}>
            <optgroup label={pickerText.keyFormats}>
              {COPY_FORMATS.map((f) => (
                <option key={f.value} value={f.value}>
                  {f.label}
                </option>
              ))}
            </optgroup>
            {cslItems && copyFormatted && (
              <optgroup label={pickerText.formattedStyles}>
                {FORMATTED_STYLES.map((s) => (
                  <option key={s.id} value={`style:${s.id}`}>
                    {s.title}
                  </option>
                ))}
              </optgroup>
            )}
          </select>
        </span>
        <button
          type="button"
          className="pin-button"
          aria-pressed={pinned}
          aria-label={pickerText.pin}
          title={pinned ? pickerText.unpinTitle : pickerText.pinTitle}
          onClick={togglePin}
        >
          <svg aria-hidden="true" viewBox="0 0 24 24" fill={pinned ? "currentColor" : "none"} stroke="currentColor">
            <path d="M9 4h6l-1 5 4 4H6l4-4z" />
            <path d="M12 13v7" />
          </svg>
        </button>
      </header>
      <div className="notices">
        {library?.settings_notice && <p role="alert">{library.settings_notice}</p>}
        {busy && <p role="status">{loadingMessage}</p>}
        {library?.error && <p role="alert">{libraryMessages.openError(library.name, library.error, library.zotero)}</p>}
        {(Object.keys(failures) as Operation[]).filter((operation) => failures[operation]).map((operation) => (
          <p role="alert" key={operation}>{FAILURE_MESSAGES[operation]}</p>
        ))}
        {error && <p role="alert">{error}</p>}
        {copyError && <p role="alert">{copyError}</p>}
        {copied && (
          <div role="status" className="copy-notice">
            <p>
              {pickerText.copied} <code>{copied.text}</code>
            </p>
            {copied.warnings.length > 0 && (
              <>
                <button type="button" className="dismiss" aria-label={pickerText.dismissWarnings} onClick={() => setCopied(null)}>
                  {pickerText.dismiss}
                </button>
                <ul aria-label={pickerText.copyWarnings}>
                  {copied.warnings.map((w) => (
                    <li key={w}>{w}</li>
                  ))}
                </ul>
              </>
            )}
          </div>
        )}
      </div>
      <p className="visually-hidden" role="status" aria-label={pickerText.citationSelection} aria-live="polite" aria-atomic="true">
        {selectionChanged ? pickerText.selectionCount(selected.size) : ""}
      </p>
      {selected.size > 0 && (
        <ul aria-label={pickerText.selectedReferences}>
          {[...selected.entries()].map(([key, r]) => (
            <li key={key}>
              <code>{r.key}</code>{" "}
              <button type="button" onClick={() => toggle(r)} aria-label={pickerText.removeReference(r.key)}>
                {pickerText.remove}
              </button>
            </li>
          ))}
        </ul>
      )}
      {style && chosen.length > 0 && (
        <section aria-label={pickerText.preview} className="format-preview">
          {shown === null && <p role="status">{pickerText.previewPending}</p>}
          {shown === "failed" && <p role="alert">{pickerText.previewError}</p>}
          {shown && shown !== "failed" && (
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
      )}
      <div className="results-area">
        <div className="results-heading" aria-hidden="true">
          <span>{query ? pickerText.bestMatches : pickerText.references}</span><span>{results.length > 0 ? pickerText.resultCount(results.length) : ""}</span>
        </div>
        <p className="visually-hidden" role="status" aria-label={pickerText.searchResults} aria-live="polite" aria-atomic="true">
          {announcement}
        </p>
        <ul id="results" role="listbox" aria-label={pickerText.results}>
          {results.map((r, i) => {
            const why = hiddenMatch(r.matched);
            return (
              <li
                key={idKey(r.id)}
                id={`result-${i}`}
                role="option"
                aria-selected={i === active}
                data-citation-selected={selected.has(idKey(r.id)) || undefined}
                data-active={i === active || undefined}
                onClick={() => {
                  setActive(i);
                  toggle(r);
                }}
              >
                <span className="result-line">
                  <strong className="result-title">{r.title}</strong>
                  <span className="result-year">{r.year ?? pickerText.noDate}</span>
                </span>
                <span className="result-meta">
                  <span className="result-authors">{r.authors}</span>
                  <code className="result-key">{r.key}</code>
                  {severalLibraries && libraryOf(r.id.source) && <span className="result-library">{libraryOf(r.id.source)}</span>}
                </span>
                {why && <span className="result-why">{pickerText.matchedIn(why)}</span>}
                {selected.has(idKey(r.id)) && <span className="visually-hidden">{pickerText.addedToCitation}</span>}
              </li>
            );
          })}
        </ul>
        {results.length === 0 && !error && library !== undefined && !busy && !library?.error && (
          <div className="empty-state">
            {library?.name ? (
              library.has_snapshot && library.references === 0
                ? <>
                  <p>{libraryMessages.empty(library.snapshot_name, library.zotero)}</p>
                  <button type="button" onClick={() => void perform("choose", chooseLibrary)}>{pickerText.chooseLibrary}</button>
                </>
                : query
                  ? <><p>{pickerText.noMatches}</p><p className="empty-hint">{pickerText.noMatchesHint}</p></>
                  : <><p>{pickerText.typeToFind}</p><p className="empty-hint">{pickerText.typeToFindHint}</p></>
            ) : library?.state === "none" ? (
              <>
                <p>{pickerText.firstRun}</p>
                <button type="button" aria-label={pickerText.firstRunChoose} onClick={() => void perform("choose", chooseLibrary)}>
                  {pickerText.chooseLibrary}
                </button>{" "}
                <button type="button" title={pickerText.useZoteroTitle} onClick={() => void perform("zotero", useZotero)}>
                  {pickerText.useZotero}
                </button>
              </>
            ) : null}
          </div>
        )}
      </div>
      <footer>
        <section aria-label={pickerText.libraryRegion} className="library-bar">
          <span className="status-dot" data-health={health} aria-hidden="true" />
          <span className="library-name" role="status" aria-label={pickerText.libraryStatus} aria-live="polite" aria-atomic="true">
            {libraryMessages.summary(library)}
          </span>
          <small className="key-hints">
            <kbd>↵</kbd>{pickerText.keyHints.copy}<kbd>⇧↵</kbd>{pickerText.keyHints.add}<kbd>Esc</kbd>{pickerText.keyHints.hide}
          </small>
          <span className="more">
            <button
              type="button"
              ref={moreButton}
              className="quiet-button"
              aria-haspopup="menu"
              aria-keyshortcuts={isMac ? "Meta+K" : "Control+K"}
              aria-expanded={menuOpen}
              aria-controls={menuOpen ? "more-menu" : undefined}
              title={pickerText.moreTitle(modifier)}
              onClick={() => (menuOpen ? closeMenu(true) : setMenuOpen(true))}
            >
              {pickerText.more} <kbd aria-hidden="true">{modifier}K</kbd>
            </button>
            {menuOpen && (
              <MoreMenu onClose={closeMenu}>
                <MenuItem label={pickerText.chooseLibrary} title={pickerText.chooseTitle} onSelect={() => runFromMenu("choose", chooseLibrary)} />
                {!library?.zotero && (
                  <MenuItem label={pickerText.useZotero} title={pickerText.useZoteroTitle} onSelect={() => runFromMenu("zotero", useZotero)} />
                )}
                {library?.zotero && useZoteroLibraries && (
                  <div role="group" aria-label={pickerText.zoteroLibraries}>
                    {(["user" as const, ...offeredGroups.map((id) => ({ group: id }))] as ZoteroLibrary[]).map((choice) => {
                      const checked = isChosen(choice);
                      const last = checked && chosenLibraries.length === 1;
                      return (
                        <MenuItem key={typeof choice === "object" ? choice.group : choice}
                          label={typeof choice === "object" ? groupName(choice.group) : pickerText.myLibrary}
                          title={last ? pickerText.keepOneLibrary : undefined} checked={checked} disabled={last}
                          onSelect={() => toggleLibrary(choice)} />
                      );
                    })}
                  </div>
                )}
                {library?.name && (
                  <MenuItem label={pickerText.reload} title={pickerText.reloadTitle} onSelect={() => runFromMenu("reload", reloadLibrary)} />
                )}
                <MenuItem label={pickerText.density(densityNames[density])} title={pickerText.densityTitle} onSelect={changeDensity} />
                <MenuItem label={pickerText.largerText} checked={largerText} onSelect={toggleLargerText} />
                {/* Closing the window only hides it; this ends the app and frees the shortcut. */}
                <MenuItem label={pickerText.quitLabel} keys={`${modifier}Q`} onSelect={() => runFromMenu("quit", quit)} />
              </MoreMenu>
            )}
          </span>
          {library?.note && <p role="status">{library.note}</p>}
        </section>
      </footer>
    </main>
  );
}
