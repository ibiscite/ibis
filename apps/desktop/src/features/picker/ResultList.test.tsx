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
  expect(onChoose).toHaveBeenCalledWith(0, record, "select");
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
  expect(newest).toHaveBeenCalledWith(3, rows[3], "select");
  const toggle = vi.fn();
  rerender(list(0, newest, toggle));
  const tile = container.querySelector("#result-2 .tile-button")!;
  expect(tile.getAttribute("data-variant")).toBe("icon");
  expect(tile.getAttribute("aria-hidden")).toBe("true");
  expect(tile.hasAttribute("tabindex")).toBe(false);
  fireEvent.click(tile);
  expect(toggle).toHaveBeenCalledWith(2, rows[2]);
  expect(newest).toHaveBeenCalledTimes(1);
});


test.each([2, 60])("a cancelled tile drag cannot fall through to copying its row (%i px)", (distance) => {
  const choose = vi.fn(), toggle = vi.fn();
  const { container } = render(list(0, choose, toggle));
  const row = container.querySelector("#result-2")!;
  fireEvent.pointerDown(row.querySelector(".tile-button")!, { pointerType: "mouse", pointerId: 1, button: 0, clientX: 20, clientY: 20 });
  fireEvent.pointerUp(row, { pointerType: "mouse", pointerId: 1, button: 0, clientX: 20 + distance, clientY: 20 });
  fireEvent.click(row, { clientX: 20 + distance, clientY: 20 });
  expect(toggle).not.toHaveBeenCalled();
  expect(choose).not.toHaveBeenCalled();
});

test("the second mouse press prevents native word selection before double-click copy", () => {
  const { getAllByRole } = render(list(0));
  expect(fireEvent.mouseDown(getAllByRole("option")[0]!, { detail: 2, button: 0 })).toBe(false);
  expect(fireEvent.mouseDown(getAllByRole("option")[0]!, { detail: 1, button: 0 })).toBe(true);
});
