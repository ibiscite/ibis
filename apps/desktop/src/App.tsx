import { useEffect, useRef, useState } from "react";
import { hidePicker, onPickerShown, quit, search, shortcutStatus, type SearchResult, type ShortcutStatus } from "./api";

// Scaffold view: a search box wired to the core search through the Rust
// side. The full picker interface (selection, locators, copy) follows in
// later changes.
export function App() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [shortcut, setShortcut] = useState<ShortcutStatus | null>(null);
  const input = useRef<HTMLInputElement>(null);

  // When the picker is shown, put the cursor in the search box with the
  // previous query selected, so typing replaces it.
  useEffect(() => {
    const focusSearch = () => {
      input.current?.focus();
      input.current?.select();
    };
    const unlisten = onPickerShown(focusSearch);
    shortcutStatus()
      .then(setShortcut)
      .catch(() => setShortcut(null));
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, []);

  useEffect(() => {
    let current = true;
    search(query)
      .then((found) => current && (setResults(found), setError(null)))
      .catch((e: unknown) => {
        // Results of an earlier query must not sit beside this query's error.
        if (current) {
          setResults([]);
          setError(String(e));
        }
      });
    return () => {
      current = false;
    };
  }, [query]);

  return (
    <main
      onKeyDown={(e) => {
        if (e.key === "Escape" && !e.nativeEvent.isComposing) {
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
        ref={input}
        autoFocus
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
      {error && <p role="alert">{error}</p>}
      <ul aria-label="Results">
        {results.map((r) => (
          <li key={r.key}>
            <strong>{r.title}</strong> {r.authors} {r.year ?? "n.d."} <code>{r.key}</code>
          </li>
        ))}
      </ul>
      <footer>
        {/* Closing the window only hides it; this ends the app and frees the shortcut. */}
        <button type="button" onClick={() => void quit()}>
          Quit Ibis
        </button>
      </footer>
    </main>
  );
}
