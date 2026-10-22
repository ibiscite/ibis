import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import type { Density, SearchResult } from "../../api";
import { rowText } from "../../messages";
import { ResultList } from "./ResultList";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
const styles = readFileSync(new FileURL("../../styles.css", import.meta.url), "utf8");

const rows: SearchResult[] = Array.from({ length: 10 }, (_, i) => ({
  revision: "1", id: { source: "refs.bib", native_key: `k${i}` }, key: `k${i}`, title: `Title ${i}`, authors: "Author", year: 2020,
  matched: ["title"], first_author: "Author", author_count: 1, container: null, item_type: "book",
}));

test("metadata reflow measures the space left beside the year", () => {
  // A 464px spacious row with Web page, a key, library and No date has only
  // 336px for metadata: the whole row's width must not force it onto one line.
  expect(styles).toMatch(/\.result-body \{[^}]*container-type: inline-size/);
  expect(styles).not.toMatch(/\[role="option"\] \{ container-type: inline-size/);
});

function list(active: number, onChoose = vi.fn(), onToggle = vi.fn()) {
  return <ResultList results={rows} active={active} selected={new Map()} density="comfortable" libraryOf={() => null}
    onMouseMove={() => undefined} onChoose={onChoose} onToggle={onToggle} />;
}

test.each<Density>(["compact", "comfortable", "spacious"])("%s rows reserve the trailing column for the year", (density) => {
  const record = { ...rows[0]!, title: "多言語の研究 · الذاكرة", key: "memory2020", year: null };
  const { container, getByRole } = render(<ResultList results={[record]} active={0} selected={new Map()} density={density}
    libraryOf={() => "Example library"} onMouseMove={vi.fn()} onChoose={vi.fn()} onToggle={vi.fn()} />);
  const row = getByRole("option");
  const year = container.querySelector(".result-year")!;
  const key = container.querySelector(".result-key")!;
  expect(year.parentElement).toBe(row);
  expect(year.textContent).toBe("No date");
  expect(container.querySelector(".result-meta")?.contains(key)).toBe(true);
  expect(key.getAttribute("dir")).toBe("ltr");
  expect(key.getAttribute("title")).toBe(record.key);
  expect(getByRole("option", { name: /多言語の研究 · الذاكرة.*memory2020.*No date/ })).toBe(row);
});

test.each(["", "研究_memory_".repeat(1_000)])("unusual keys stay literal and do not change row actions (%#)", (key) => {
  const record = { ...rows[0]!, key, year: 0 };
  const onChoose = vi.fn();
  const { container, getByRole } = render(<ResultList results={[record]} active={0} selected={new Map()} density="comfortable"
    libraryOf={() => null} onMouseMove={vi.fn()} onChoose={onChoose} onToggle={vi.fn()} />);
  expect(container.querySelector(".result-key")?.textContent).toBe(key);
  expect(container.querySelector(".result-year")?.textContent).toBe("0");
  const range = document.createRange();
  range.selectNodeContents(container.querySelector(".result-key")!);
  window.getSelection()?.addRange(range);
  fireEvent.click(container.querySelector(".result-key")!);
  if (key) expect(onChoose).not.toHaveBeenCalled();
  window.getSelection()?.removeAllRanges();
  onChoose.mockClear();
  fireEvent.click(container.querySelector(".result-year")!);
  expect(onChoose).toHaveBeenCalledWith(0, record, "copy");
  expect(getByRole("option").getAttribute("aria-selected")).toBe("true");
});

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
