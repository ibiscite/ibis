import { invoke } from "@tauri-apps/api/core";
import { expect, it, vi } from "vitest";
import { isSnapshotChanged, newerUpdate, search } from "./api";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue([]) }));

it("sends scope changes as fresh requests on the same interface page", async () => {
  await search("研究", 7);
  await search("研究", 7, []);
  await search("研究", 7, ["zotero:group:7"]);
  const calls = vi.mocked(invoke).mock.calls;
  expect(calls).toHaveLength(3);
  const args = calls.map(([command, args]) => {
    expect(command).toBe("search");
    return args as { query: string; limit: number; scope: string[] | null; page: number; request: number };
  });
  expect(args.map(({ query, limit, scope }) => ({ query, limit, scope }))).toEqual([
    { query: "研究", limit: 7, scope: null },
    { query: "研究", limit: 7, scope: [] },
    { query: "研究", limit: 7, scope: ["zotero:group:7"] },
  ]);
  expect(new Set(args.map(({ page }) => page)).size).toBe(1);
  expect(args.map(({ request }) => request)).toEqual([1, 2, 3]);
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
