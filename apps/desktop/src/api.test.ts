import { invoke } from "@tauri-apps/api/core";
import { listen, type Event } from "@tauri-apps/api/event";
import { expect, it, vi } from "vitest";
import { isSnapshotChanged, newerUpdate, onWindowSizeSaved, previewFileCitation, search } from "./api";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue([]) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

it("a delayed size-save result cannot restore an older failure or clear a newer one", async () => {
  let deliver: (event: Event<{ revision: number; saved: boolean }>) => void = () => {};
  const stop = vi.fn();
  vi.mocked(listen).mockImplementation(async (_name, handler) => {
    deliver = handler;
    return stop;
  });
  const shown = vi.fn();
  const unlisten = await onWindowSizeSaved(shown);
  for (const [revision, saved] of [[2, true], [1, false], [3, false], [2, true], [3, true]] as const) {
    deliver({ event: "picker:window-saved", id: 1, payload: { revision, saved } });
  }
  expect(shown.mock.calls).toEqual([[true], [false]]);
  unlisten();
  expect(stop).toHaveBeenCalledOnce();
});

it("aborts only the numbered preview and removes abort listeners after completion", async () => {
  const pending: Array<() => void> = [];
  vi.mocked(invoke).mockClear().mockImplementation(() => new Promise((resolve) => pending.push(() => resolve([]))));
  const old = new AbortController();
  const current = new AbortController();
  const first = previewFileCitation([], "pandoc", null, "synthetic", old.signal);
  const second = previewFileCitation([], "pandoc", null, "synthetic", current.signal);
  const calls = vi.mocked(invoke).mock.calls;
  const firstIntent = (calls[0]![1] as { intent: object }).intent;
  const secondIntent = (calls[1]![1] as { intent: { request: number } }).intent;
  old.abort();
  expect(calls[2]).toEqual(["preview_file_citation", expect.objectContaining({ intent: { ...firstIntent, cancel: true } })]);
  expect(secondIntent.request).toBe((firstIntent as { request: number }).request + 1);
  pending.forEach((resolve) => resolve());
  await Promise.all([first, second]);
  const count = calls.length;
  current.abort();
  expect(calls).toHaveLength(count);
  vi.mocked(invoke).mockResolvedValue([]);
});

it("sends scope changes as fresh requests on the same interface page", async () => {
  // The interface page and counter live for the module's lifetime, across
  // searches (and tests). Exercise a prior search rather than assuming this
  // test runs first, and inspect only this scope-changing sequence's calls.
  await search("Earlier synthetic query");
  const previous = vi.mocked(invoke).mock.lastCall![1] as { request: number };
  vi.mocked(invoke).mockClear();
  await search("研究", 7);
  await search("研究", 7, []);
  await search("研究", 7, ["zotero:group:7"]);
  const calls = vi.mocked(invoke).mock.calls;
  expect(calls).toHaveLength(3);
  const args = calls.map(([command, args]) => {
    expect(command).toBe("search");
    return args as { query: { text: string; interpret_locators: boolean }; limit: number; scope: string[] | null; page: number; request: number };
  });
  expect(args.map(({ query, limit, scope }) => ({ query, limit, scope }))).toEqual([
    { query: { text: "研究", interpret_locators: true }, limit: 7, scope: null },
    { query: { text: "研究", interpret_locators: true }, limit: 7, scope: [] },
    { query: { text: "研究", interpret_locators: true }, limit: 7, scope: ["zotero:group:7"] },
  ]);
  expect(new Set(args.map(({ page }) => page)).size).toBe(1);
  expect(args.map(({ request }) => request)).toEqual([1, 2, 3].map((offset) => previous.request + offset));
});

it("recognizes a changed library by its error code, not its wording", () => {
  expect(isSnapshotChanged({ code: "snapshot_changed" })).toBe(true);
  expect(isSnapshotChanged({ code: "failed", message: "library snapshot changed" })).toBe(false);
  expect(isSnapshotChanged("library snapshot changed")).toBe(false);
  expect(isSnapshotChanged(null)).toBe(false);
  expect(isSnapshotChanged(undefined)).toBe(false);
});

it("retains only newer update attempts and serial revisions, including rollover", () => {
  const old = { attempt: 2, revision: 0xffffffff, received: 1, total: null, phase: "downloading" as const };
  expect(newerUpdate(null, old)).toBe(old);
  expect(newerUpdate(old, { ...old, attempt: 1, revision: 100 })).toBe(old);
  expect(newerUpdate(old, { ...old })).toBe(old);
  const wrapped = { ...old, revision: 0 };
  expect(newerUpdate(old, wrapped)).toBe(wrapped);
  expect(newerUpdate(wrapped, old)).toBe(wrapped);
  expect(newerUpdate(old, { ...old, attempt: 3, revision: 1 }).attempt).toBe(3);
});

it("can search the original query literally without interpreting locators", async () => {
  await search("Smith p.12", 20, null, false);
  expect(vi.mocked(invoke).mock.lastCall).toEqual(["search", expect.objectContaining({
    query: { text: "Smith p.12", interpret_locators: false },
  })]);
});
