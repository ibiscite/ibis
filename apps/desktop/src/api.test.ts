import { invoke } from "@tauri-apps/api/core";
import { expect, it, vi } from "vitest";
import { isSnapshotChanged, search } from "./api";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue([]) }));

it("sends scope changes as fresh requests on the same interface page", async () => {
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
  expect(args.map(({ request }) => request)).toEqual([1, 2, 3]);
});

it("recognizes a changed library by its error code, not its wording", () => {
  expect(isSnapshotChanged({ code: "snapshot_changed" })).toBe(true);
  expect(isSnapshotChanged({ code: "failed", message: "library snapshot changed" })).toBe(false);
  expect(isSnapshotChanged("library snapshot changed")).toBe(false);
  expect(isSnapshotChanged(null)).toBe(false);
  expect(isSnapshotChanged(undefined)).toBe(false);
});

it("can search the original query literally without interpreting locators", async () => {
  await search("Smith p.12", 20, null, false);
  expect(vi.mocked(invoke).mock.lastCall).toEqual(["search", expect.objectContaining({
    query: { text: "Smith p.12", interpret_locators: false },
  })]);
});
