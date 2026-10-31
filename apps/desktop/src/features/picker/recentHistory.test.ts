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
