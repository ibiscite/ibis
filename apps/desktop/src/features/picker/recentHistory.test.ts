import { expect, test, vi } from "vitest";
import { readHistory, rememberCitation, recentIds } from "./recentHistory";

test("history is scoped to the originating app, deduplicated and bounded, and stores only identities", () => {
  const storage = { getItem: () => null, setItem: vi.fn() };
  let history = readHistory(storage);
  const a = { library: "file:synthetic", key: "a" }, b = { library: "file:synthetic", key: "b" };
  history = rememberCitation(history, "invented.writer", [a, b], storage);
  history = rememberCitation(history, "invented.editor", [b], storage);
  history = rememberCitation(history, "invented.writer", [a], storage);
  expect(recentIds(history, "invented.writer")).toEqual([a, b]);
  expect(recentIds(history, "invented.editor")).toEqual([b]);
  expect(recentIds(history, null)).toEqual([]);
  expect(JSON.parse(storage.setItem.mock.calls.at(-1)![1])).toEqual(history);
});

test("failed I/O and malformed stored data never invent rows or interrupt a copy", () => {
  const storage = { getItem: () => '{"private":"invented"}', setItem: () => { throw Error("blocked"); } };
  expect(readHistory(storage)).toEqual([]);
  expect(() => rememberCitation([], "invented.writer", [{ library: "file:synthetic", key: "a" }], storage)).not.toThrow();
});

test("two unknown destinations never share history presented as Cited here recently", () => {
  const storage = { getItem: () => null, setItem: vi.fn() };
  const history = rememberCitation([], null, [{ library: "file:synthetic", key: "a" }], storage);
  expect(recentIds(history, null)).toEqual([]);
  expect(storage.setItem).not.toHaveBeenCalled();
});

test("large identities stay within the read budget without mutating older history", () => {
  let saved = "";
  const storage = { getItem: () => saved || null, setItem: (_key: string, value: string) => { saved = value; } };
  let history = readHistory(storage);
  for (let app = 0; app < 20; app++) history = rememberCitation(history, `invented.${app}`, Array.from({ length: 30 }, (_, item) => ({ library: "f".repeat(512), key: `k${item}`.padEnd(512, "x") })), storage);
  const before = JSON.stringify(history);
  expect(saved.length).toBeLessThanOrEqual(256 * 1024);
  expect(readHistory(storage)).toEqual(history);
  rememberCitation(history, "invented.new", [{ library: "file:synthetic", key: "fresh" }], storage);
  expect(JSON.stringify(history)).toBe(before);
});
