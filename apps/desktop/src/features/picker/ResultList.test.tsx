import { recordId } from "../../test-fixtures";
import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import type { SearchResult } from "../../api";
import { ResultList } from "./ResultList";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

const rows: SearchResult[] = Array.from({ length: 10 }, (_, i) => ({
  revision: "1", id: { library: "refs.bib", key: recordId(`k${i}`) }, key: `k${i}`, cite: "1", source: "refs.bib", title: `Title ${i}`, authors: "Author", year: "2020",
  matched: ["title"], partial: false, first_author: "Author", author_count: 1, container: null, item_type: "book",
}));

function list(active: number, onChoose = vi.fn(), onToggle = vi.fn()) {
  return <ResultList results={rows} active={active} selected={new Map()} density="comfortable" libraryOf={() => null}
    onMouseMove={() => undefined} onChoose={onChoose} onToggle={onToggle} />;
}

test("a closest or spelling match says so on its own row, and its name reads with pauses", () => {
  const { getAllByRole } = render(<ResultList results={[{ ...rows[0]!, partial: true, year: "2021", authors: "Müller" }, rows[1]!, { ...rows[2]!, typo: true }]} active={0} selected={new Map()} density="comfortable"
    libraryOf={() => null} onMouseMove={() => undefined} onChoose={vi.fn()} onToggle={vi.fn()} />);
  const [closest, exact, spelled] = getAllByRole("option");
  expect(closest!.textContent).toContain("Closest match");
  expect(exact!.textContent).not.toMatch(/Closest match|Spelling match/);
  expect(spelled!.textContent).toContain("Spelling match");
  expect(closest!.textContent).toMatch(/2021, Müller/);
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
