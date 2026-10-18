import type React from "react";
import * as nativeApi from "./api";
import { libraryMessages, operationFailures as FAILURE_MESSAGES, pickerText } from "./messages";
import { useEffect, useRef, useState } from "react";
import {
  COPY_FORMATS,
  type Copied,
  type CopyFormat,
  type LibraryStatus,
  type RecordId,
  type SearchResult,
  type ShortcutStatus,
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

// A stable provider lets the development gallery use synthetic data without IPC.
export type PickerApi = Pick<typeof nativeApi,
  "chooseLibrary" | "libraryStatus" | "reloadLibrary" | "copyCitation" | "copyFormat" |
  "hidePicker" | "onLibraryChanged" | "onPickerShown" | "quit" | "search" | "shortcutStatus">;

export type PickerPreview = { query?: string; copied?: Copied };

// The picker: search, choose one or more references, copy them as a citation.
export function App({ api = nativeApi, preview }: { api?: PickerApi; preview?: PickerPreview } = {}) {
  const { chooseLibrary, libraryStatus, reloadLibrary, copyCitation, copyFormat,
    hidePicker, onLibraryChanged, onPickerShown, quit, search, shortcutStatus } = api;
  const [query, setQuery] = useState(preview?.query ?? "");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [announcement, setAnnouncement] = useState("");
  // The query the shown results belong to; differs while a search is pending.
  const [resultsQuery, setResultsQuery] = useState<string | null>(null);
  const [active, setActive] = useState(0);
  const [selected, setSelected] = useState<Map<string, SearchResult>>(new Map());
  const [selectionChanged, setSelectionChanged] = useState(false);
  const [format, setFormat] = useState<CopyFormat>("pandoc");
  const [copied, setCopied] = useState<Copied | null>(preview?.copied ?? null);
  const [error, setError] = useState<string | null>(null);
  const [copyError, setCopyError] = useState<string | null>(null);
  const [shortcut, setShortcut] = useState<ShortcutStatus | null>(null);
  // Changes whenever a library opens, so the current query is searched again.
  const [libraryVersion, setLibraryVersion] = useState(0);
  // Undefined while loading; null means the status request itself failed.
  const [library, setLibrary] = useState<LibraryStatus | null>();
  const input = useRef<HTMLInputElement>(null);
  const [failures, setFailures] = useState<Partial<Record<Operation, boolean>>>({});
  const report = (operation: Operation, failed = true) => {
    setFailures((previous) => ({ ...previous, [operation]: failed }));
  };
  const perform = async (operation: Operation, action: () => Promise<unknown>) => {
    try { await action(); report(operation, false); }
    catch { report(operation); }
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
      input.current?.focus();
      input.current?.select();
    };
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
    copyFormat()
      .then(setFormat)
      .catch(() => report("format"));
    return () => {
      acceptInitialStatus = false;
      mounted = false;
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

  // Copy the selection, or the highlighted result when nothing is selected.
  const copy = async () => {
    // Never copy a highlighted result that belongs to an earlier query.
    const current = resultsQuery === query ? results : [];
    const chosen = selected.size > 0 ? [...selected.values()] : current.slice(active, active + 1);
    if (chosen.length === 0) return;
    setCopied(null);
    setCopyError(null);
    let outcome: Copied;
    try {
      outcome = await copyCitation(
        chosen.map((r) => r.id),
        format,
      );
    } catch {
      setCopyError(pickerText.copyError);
      return;
    }
    setCopied(outcome);
    setSelected(new Map());
    // Get out of the way so the user can paste, unless there is something to read.
    if (outcome.warnings.length === 0) void perform("hide", hidePicker);
  };

  const busy = library?.state === "loading" || library?.state === "reloading";
  const loadingMessage = library?.state === "reloading"
    ? libraryMessages.reloading(library.name)
    : libraryMessages.opening(library?.name ?? null);

  return (
    <main
      onKeyDown={(e) => {
        if (composing(e)) return;
        if (e.key === "Escape") {
          e.preventDefault();
          void perform("hide", hidePicker);
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
      <div className="search-bar" data-tauri-drag-region title={pickerText.dragSearchBar}>
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
      </div>
      <div className="notices">
        {library?.settings_notice && <p role="alert">{library.settings_notice}</p>}
        {busy && <p role="status">{loadingMessage}</p>}
        {library?.error && <p role="alert">{libraryMessages.openError(library.name, library.error)}</p>}
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
      <div className="results-area">
        <div className="results-heading" aria-hidden="true">
          <span>{pickerText.references}</span><span>{results.length > 0 ? pickerText.resultCount(results.length) : ""}</span>
        </div>
        <p className="visually-hidden" role="status" aria-label={pickerText.searchResults} aria-live="polite" aria-atomic="true">
          {announcement}
        </p>
        <ul id="results" role="listbox" aria-label={pickerText.results}>
          {results.map((r, i) => (
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
              <span className="result-text">
                <strong>{r.title}</strong>
                <span className="result-meta">
                  {r.authors} · {r.year ?? pickerText.noDate}
                </span>
              </span>
              <code className="result-key">{r.key}</code>
              {selected.has(idKey(r.id)) && <span className="visually-hidden">{pickerText.addedToCitation}</span>}
            </li>
          ))}
        </ul>
        {results.length === 0 && !error && library !== undefined && !busy && !library?.error && (
          <div className="empty-state">
            {library?.name ? (
              <p>{library.has_snapshot && library.references === 0
                ? libraryMessages.empty(library.snapshot_name)
                : query ? pickerText.noMatches : pickerText.typeToFind}</p>
            ) : library?.state === "none" ? (
              <>
                <p>{pickerText.firstRun}</p>
                <button type="button" aria-label={pickerText.firstRunChoose} onClick={() => void perform("choose", chooseLibrary)}>
                  {pickerText.chooseLibrary}
                </button>
              </>
            ) : null}
          </div>
        )}
      </div>
      <footer>
        <section aria-label={pickerText.libraryRegion} className="library-bar">
          <span className="library-name" role="status" aria-label={pickerText.libraryStatus} aria-live="polite" aria-atomic="true">
            {libraryMessages.summary(library)}
          </span>{" "}
          <button type="button" className="quiet-button" aria-label={pickerText.chooseLibrary} title={pickerText.chooseTitle} onClick={() => void perform("choose", chooseLibrary)}>
            {pickerText.chooseShort}
          </button>
          {library?.name && (
            <button type="button" className="icon-button" aria-label={pickerText.reload} title={pickerText.reloadTitle} onClick={() => void perform("reload", reloadLibrary)}>
              ↻
            </button>
          )}
          {library?.note && <p role="status">{library.note}</p>}
        </section>
        <div className="footer-tools">
          <label className="visually-hidden" htmlFor="format">
            {pickerText.copyAs}
          </label>
          <select id="format" value={format} onChange={(e) => setFormat(e.target.value as CopyFormat)}>
            {COPY_FORMATS.map((f) => (
              <option key={f.value} value={f.value}>
                {f.label}
              </option>
            ))}
          </select>
          <small className="key-hints">
            <kbd>↵</kbd>{pickerText.keyHints.copy}<kbd>⇧↵</kbd>{pickerText.keyHints.add}<kbd>Esc</kbd>{pickerText.keyHints.hide}
          </small>
          {/* Closing the window only hides it; this ends the app and frees the shortcut. */}
          <button type="button" className="quiet-button" aria-label={pickerText.quitLabel} onClick={() => void perform("quit", quit)}>
            {pickerText.quit}
          </button>
        </div>
      </footer>
    </main>
  );
}
