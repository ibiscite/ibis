import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import type { SearchResult } from "../../api";
import { rowText } from "../../messages";
import { ResultList } from "./ResultList";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

const rows: SearchResult[] = Array.from({ length: 10 }, (_, i) => ({
  revision: "1", id: { source: "refs.bib", native_key: `k${i}` }, key: `k${i}`, title: `Title ${i}`, authors: "Author", year: 2020,
  matched: ["title"], first_author: "Author", author_count: 1, container: null, item_type: "book",
}));

function list(active: number, onChoose = vi.fn(), onToggle = vi.fn()) {
  return <ResultList results={rows} active={active} selected={new Map()} density="comfortable" libraryOf={() => null}
    onMouseMove={() => undefined} onChoose={onChoose} onToggle={onToggle} />;
}

test("moving the highlight re-renders only the rows it leaves and reaches", () => {
  const { rerender, container } = render(list(0));
  const rendered = vi.spyOn(rowText, "itemType");
  rerender(list(1));
  const keys = new Set(rendered.mock.calls.map(([type]) => type));
  expect(keys).toEqual(new Set(["book"]));
  // Two rows changed; each asks for its type in words at most twice.
  expect(rendered.mock.calls.length).toBeGreaterThan(0);
  expect(rendered.mock.calls.length).toBeLessThanOrEqual(4);
  expect(container.querySelector("[data-active]")?.id).toBe("result-1");
});

test("memoised rows call the newest handlers", () => {
  const first = vi.fn();
  const newest = vi.fn();
  const { rerender, container } = render(list(0, first));
  rerender(list(0, newest));
  fireEvent.click(container.querySelector("#result-3")!);
  expect(first).not.toHaveBeenCalled();
  expect(newest).toHaveBeenCalledWith(3, rows[3], "copy");
  const toggle = vi.fn();
  rerender(list(0, newest, toggle));
  fireEvent.click(container.querySelector("#result-2 .tile-button")!);
  expect(toggle).toHaveBeenCalledWith(2, rows[2]);
  expect(newest).toHaveBeenCalledTimes(1);
});
