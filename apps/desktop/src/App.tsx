import { useEffect, useState } from "react";
import { search, type SearchResult } from "./api";

// Scaffold view: a search box wired to the core search through the Rust
// side. The full picker interface (selection, locators, copy) follows in
// later changes.
export function App() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    search(query)
      .then((found) => current && (setResults(found), setError(null)))
      .catch((e: unknown) => current && setError(String(e)));
    return () => {
      current = false;
    };
  }, [query]);

  return (
    <main>
      <label htmlFor="search">Search references</label>
      <input
        id="search"
        type="search"
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
    </main>
  );
}
