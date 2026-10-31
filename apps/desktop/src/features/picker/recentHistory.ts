// Only opaque reference identities and application IDs are kept. Never titles,
// queries, citation text, locators or library paths. Ordinary rows are resolved
// afresh from the current engine snapshot when the empty search is shown.
type Id = { library: string; key: string };
export type RecentHistory = { app: string; items: Id[] }[];
type StoragePort = Pick<Storage, "getItem" | "setItem">;
const storageKey = "ibis-citation-history";
const validString = (value: unknown): value is string => typeof value === "string" && value.length > 0 && value.length <= 512 && !/[\u0000-\u001f\u007f-\u009f]/.test(value);
function identity(value: unknown): value is Id {
  return typeof value === "object" && value !== null && Object.keys(value).length === 2 && "library" in value && validString(value.library) && "key" in value && validString(value.key);
}
export function readHistory(storage: StoragePort): RecentHistory {
  try {
    const raw = storage.getItem(storageKey);
    if (!raw || raw.length > 256 * 1024) return [];
    const history: unknown = JSON.parse(raw);
    return Array.isArray(history) && history.length <= 20 && history.every((entry) => typeof entry === "object" && entry && Object.keys(entry).length === 2 &&
      validString(entry.app) && Array.isArray(entry.items) && entry.items.length <= 30 && entry.items.every(identity)) ? history : [];
  } catch { return []; }
}
export function recentIds(history: RecentHistory, app: string | null): Id[] {
  return app === null ? [] : history.find((entry) => entry.app === app)?.items ?? [];
}
export function rememberCitation(history: RecentHistory, app: string | null, ids: unknown[], storage: StoragePort): RecentHistory {
  if (app === null) return history;
  const items = [...ids.filter(identity), ...recentIds(history, app)].filter((id, index, all) => all.findIndex((other) => other.library === id.library && other.key === id.key) === index).slice(0, 30);
  const next = [{ app, items }, ...history.filter((entry) => entry.app !== app).map((entry) => ({ ...entry, items: [...entry.items] }))].slice(0, 20);
  let encoded = JSON.stringify(next);
  while (encoded.length > 256 * 1024) {
    const oldest = next.at(-1)!;
    if (oldest.items.length > 1) oldest.items.pop(); else next.pop();
    encoded = JSON.stringify(next);
  }
  try { storage.setItem(storageKey, encoded); } catch { /* In-memory recents still work when webview storage is unavailable. */ }
  return next;
}
