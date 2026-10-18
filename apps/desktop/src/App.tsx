import type React from "react";
import { useEffect, useRef, useState } from "react";
import {
  COPY_FORMATS,
  copyCitation,
  copyFormat,
  hidePicker,
  onLibraryChanged,
  onPickerShown,
  quit,
  search,
  shortcutStatus,
  type Copied,
  type CopyFormat,
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

// The picker: search, choose one or more references, copy them as a citation.
export function App() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  // The query the shown results belong to; differs while a search is pending.
  const [resultsQuery, setResultsQuery] = useState<string | null>(null);
  const [active, setActive] = useState(0);
  const [selected, setSelected] = useState<Map<string, SearchResult>>(new Map());
  const [format, setFormat] = useState<CopyFormat>("pandoc");
  const [copied, setCopied] = useState<Copied | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copyError, setCopyError] = useState<string | null>(null);
  const [shortcut, setShortcut] = useState<ShortcutStatus | null>(null);
  // Changes whenever a library opens, so the current query is searched again.
  const [libraryVersion, setLibraryVersion] = useState(0);
  const input = useRef<HTMLInputElement>(null);

  // When the picker is shown, put the cursor in the search box with the
  // previous query selected, so typing replaces it.
  useEffect(() => {
    const focusSearch = () => {
      input.current?.focus();
      input.current?.select();
    };
    const unlisten = onPickerShown(focusSearch);
    const unlistenLibrary = onLibraryChanged(() => setLibraryVersion((v) => v + 1));
    shortcutStatus()
      .then(setShortcut)
      .catch(() => setShortcut(null));
    copyFormat()
      .then(setFormat)
      .catch(() => undefined);
    return () => {
      void unlisten.then((stop) => stop());
      void unlistenLibrary.then((stop) => stop());
    };
  }, []);

  useEffect(() => {
    let current = true;
    search(query)
      .then((found) => {
        if (current) {
          setResults(found);
          setResultsQuery(query);
          setActive(0);
          setError(null);
        }
      })
      .catch((e: unknown) => {
        // Results of an earlier query must not sit beside this query's error.
        if (current) {
          setResults([]);
          setResultsQuery(query);
          setError(String(e));
        }
      });
    return () => {
      current = false;
    };
  }, [query, libraryVersion]);

  // Keep the highlighted result visible while moving with the arrow keys.
  useEffect(() => {
    document.getElementById(`result-${active}`)?.scrollIntoView?.({ block: "nearest" });
  }, [active]);

  const toggle = (result: SearchResult) => {
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
    } catch (e: unknown) {
      setCopyError(String(e));
      return;
    }
    setCopied(outcome);
    setSelected(new Map());
    // Get out of the way so the user can paste, unless there is something to read.
    if (outcome.warnings.length === 0) void hidePicker();
  };

  return (
    <main
      onKeyDown={(e) => {
        if (composing(e)) return;
        if (e.key === "Escape") {
          e.preventDefault();
          void hidePicker();
        } else if (e.key === "q" && (e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey) {
          e.preventDefault();
          void quit();
        }
      }}
    >
      {shortcut?.error && (
        <p role="status">
          {shortcut.registered
            ? `Your shortcut setting was not used: ${shortcut.error}.`
            : `The shortcut ${shortcut.shortcut} is not available: ${shortcut.error}.`}
        </p>
      )}
      <label htmlFor="search">Search references</label>
      <input
        id="search"
        type="search"
        role="combobox"
        aria-expanded={results.length > 0}
        aria-controls="results"
        aria-activedescendant={results[active] ? `result-${active}` : undefined}
        ref={input}
        autoFocus
        value={query}
        onChange={(e) => setQuery(e.target.value)}
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
      <label htmlFor="format">Copy as</label>
      <select id="format" value={format} onChange={(e) => setFormat(e.target.value as CopyFormat)}>
        {COPY_FORMATS.map((f) => (
          <option key={f.value} value={f.value}>
            {f.label}
          </option>
        ))}
      </select>
      {error && <p role="alert">{error}</p>}
      {copyError && <p role="alert">Nothing was copied: {copyError}</p>}
      {copied && (
        <div role="status">
          <p>
            Copied <code>{copied.text}</code>
          </p>
          {copied.warnings.length > 0 && (
            <ul aria-label="Copy warnings">
              {copied.warnings.map((w) => (
                <li key={w}>{w}</li>
              ))}
            </ul>
          )}
        </div>
      )}
      {selected.size > 0 && (
        <ul aria-label="Selected references">
          {[...selected.entries()].map(([key, r]) => (
            <li key={key}>
              <code>{r.key}</code>{" "}
              <button type="button" onClick={() => toggle(r)} aria-label={`Remove ${r.key} from the selection`}>
                Remove
              </button>
            </li>
          ))}
        </ul>
      )}
      <ul id="results" role="listbox" aria-label="Results" aria-multiselectable="true">
        {results.map((r, i) => (
          <li
            key={idKey(r.id)}
            id={`result-${i}`}
            role="option"
            aria-selected={selected.has(idKey(r.id))}
            data-active={i === active || undefined}
            onClick={() => {
              setActive(i);
              toggle(r);
            }}
          >
            <strong>{r.title}</strong> {r.authors} {r.year ?? "n.d."} <code>{r.key}</code>
          </li>
        ))}
      </ul>
      <footer>
        <small>Enter copies · Shift+Enter adds to the selection · Escape hides</small>{" "}
        {/* Closing the window only hides it; this ends the app and frees the shortcut. */}
        <button type="button" onClick={() => void quit()}>
          Quit Ibis
        </button>
      </footer>
    </main>
  );
}
