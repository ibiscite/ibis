// Typed wrappers around the commands exposed by the Rust side
// (`src-tauri/src/main.rs`). The UI never calls `invoke` directly.
import { invoke } from "@tauri-apps/api/core";

export interface LibraryStatus {
  path: string | null;
  references: number;
  warnings: number;
}

export interface SearchResult {
  key: string;
  title: string;
  authors: string;
  year: number | null;
  matched: string[];
}

export function openLibrary(path: string): Promise<LibraryStatus> {
  return invoke<LibraryStatus>("open_library", { path });
}

export function search(query: string, limit = 20): Promise<SearchResult[]> {
  return invoke<SearchResult[]>("search", { query, limit });
}
