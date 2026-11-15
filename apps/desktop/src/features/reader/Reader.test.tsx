import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { useState } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { App } from "../../App";
import { backend, displayTree, findInCitation, inCitation, receipt, removeFromCitation, resetApp, result, type PreviewRequest } from "../../testing/app";
import { Reader } from "./Reader";
import { inventedSource, PageSources, type PageSource } from "./source";

afterEach(async () => { await resetApp(); vi.unstubAllGlobals(); });

const book: PageSource = { id: "duplicate-labels", labels: ["xi", "xii", "xii", "A-3", "xiv"], draw: () => () => {} };

// The invented book: xi, xii, two unnumbered plates, then 415 onwards.
function Harness({ initial = "", source = inventedSource() }: { initial?: string; source?: PageSource }) {
  const [page, setPage] = useState(initial);
  return <>
    <input aria-label="Page" value={page} onChange={(event) => setPage(event.target.value)} />
    <Reader source={source} title="Invented title" page={page} onPageChange={setPage} />
  </>;
}
const strip = () => screen.getByRole("listbox", { name: "Pages" });
const thumb = (name: string) => within(strip()).getByRole("option", { name });
const page = () => (screen.getByRole("textbox", { name: "Page" }) as HTMLInputElement).value;
const selected = () => within(strip()).getAllByRole("option").map((option) => option.getAttribute("aria-selected"));
const candidate = (position: number) => within(strip()).getByRole("option", { name: new RegExp(`PDF page ${position} of 5`) });

function observePages() {
  let observe!: IntersectionObserverCallback;
  vi.stubGlobal("IntersectionObserver", class {
    constructor(callback: IntersectionObserverCallback, options?: IntersectionObserverInit) { if (options?.root) observe = callback; }
    observe() {} disconnect() {}
  });
  return (position: number) => {
    const target = screen.getByRole("group", { name: new RegExp(`PDF page ${position} of 5`) });
    const rect = target.getBoundingClientRect();
    act(() => observe([{ target, isIntersecting: true, intersectionRatio: 1, boundingClientRect: rect,
      intersectionRect: rect, rootBounds: null, time: 0 }], {} as IntersectionObserver));
  };
}

test("duplicate xii labels leave both physical candidates unselected", () => {
  render(<Harness source={book} initial="xii" />);
  expect(selected()).toEqual(["false", "false", "false", "false", "false"]);
  expect(page()).toBe("xii");
});

test("candidate controls distinguish PDF positions and a second choice survives the controlled rerender", () => {
  const view = render(<Harness source={book} initial="xii" />);
  expect(candidate(2).textContent).toContain("xii");
  fireEvent.click(candidate(3));
  view.rerender(<Harness source={book} initial="xii" />);
  expect(page()).toBe("xii");
  expect(selected()).toEqual(["false", "false", "true", "false", "false"]);
  fireEvent.change(screen.getByRole("textbox", { name: "Page" }), { target: { value: "xi" } });
  expect(thumb("xi").getAttribute("aria-selected")).toBe("true");
  fireEvent.change(screen.getByRole("textbox", { name: "Page" }), { target: { value: "xii" } });
  expect(selected().every((value) => value === "false")).toBe(true);
});

test("an unknown numeric label never selects its physical PDF position", () => {
  const source = { ...book, labels: Array.from({ length: 20 }, (_, index) => String(index + 417)) };
  render(<Harness source={source} initial="12" />);
  expect(selected()).toHaveLength(20);
  expect(selected().every((value) => value === "false")).toBe(true);
  expect(page()).toBe("12");
});

test.each(["xii–xiv", "xi–xii"])("%s never guesses a physical PDF position", (initial) => {
  render(<Harness source={book} initial={initial} />);
  expect(selected().every((value) => value === "false")).toBe(true);
  expect(page()).toBe(initial);
});

test("an exact A-3 label resolves before range parsing", () => {
  render(<Harness source={book} initial="A-3" />);
  expect(thumb("A-3").getAttribute("aria-selected")).toBe("true");
});

test("scrolling changes the page in view without resolving or changing the citation", () => {
  const scroll = observePages();
  render(<Harness source={book} initial="xii" />);
  scroll(5);
  expect(within(screen.getByRole("group", { name: "Page in view" })).getByText(/xiv/)).toBeTruthy();
  expect(page()).toBe("xii");
  expect(selected().every((value) => value === "false")).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "Cite This Page" }));
  expect(page()).toBe("xiv");
  expect(thumb("xiv").getAttribute("aria-selected")).toBe("true");
});

test("the keyboard extends an explicit choice and modifier clicks can remove it", () => {
  const scroll = observePages();
  render(<Harness source={book} initial="xii" />);
  fireEvent.click(candidate(3));
  scroll(5);
  fireEvent.keyDown(screen.getByLabelText("Pages of the PDF"), { key: "Enter", ctrlKey: true, shiftKey: true });
  expect(page()).toBe("xii, A-3, xiv");
  expect(selected()).toEqual(["false", "false", "true", "true", "true"]);
  fireEvent.click(candidate(3), { metaKey: true });
  expect(page()).toBe("A-3, xiv");
  expect(selected()).toEqual(["false", "false", "false", "true", "true"]);
});

test("confirming an unnumbered page preserves an explicitly chosen duplicate", () => {
  render(<Harness source={{ ...book, labels: ["xi", "xii", "xii", null, "xiv"] }} />);
  fireEvent.click(candidate(3));
  fireEvent.click(thumb("PDF 4"), { metaKey: true });
  expect(page()).toBe("xii");
  fireEvent.change(screen.getByRole("textbox", { name: "What number is printed on PDF page 4?" }), { target: { value: "42" } });
  fireEvent.click(screen.getByRole("button", { name: "Use for Nearby Pages" }));
  expect(page()).toBe("xii, 42");
  expect(selected()).toEqual(["false", "false", "true", "true", "false"]);
});

test("source labels containing control characters stay unnumbered", () => {
  render(<Harness source={{ ...book, labels: ["xii\n", "xii", "417\t"] }} initial="xii" />);
  expect(thumb("PDF 1").getAttribute("aria-selected")).toBe("false");
  expect(thumb("xii").getAttribute("aria-selected")).toBe("true");
  fireEvent.click(thumb("PDF 3"));
  expect(screen.getByRole("textbox", { name: "What number is printed on PDF page 3?" })).toBeTruthy();
});

test("the production citation owner isolates items sharing a PDF, switches items and retires a removed owner", async () => {
  const previews: PreviewRequest[] = [];
  backend({ search: () => [result("a", "First work"), result("b", "Second work")],
    preview_file_citation: (request) => {
      previews.push(request);
      const spans = request.items.map((_, item) => ({ item, text: `(${item + 1})` }));
      return receipt(spans.map((span) => span.text).join(""), displayTree(spans));
    } });
  render(<PageSources.Provider value={() => book}><App /></PageSources.Provider>);
  fireEvent.click(await screen.findByText("First work"), { ctrlKey: true });
  fireEvent.click(screen.getByText("Second work"), { ctrlKey: true });
  const open = async (key: string) => {
    fireEvent.click(await findInCitation(key));
    await screen.findByRole("textbox", { name: "Page" });
  };
  const read = async () => {
    fireEvent.click(screen.getByRole("button", { name: "Show PDF" }));
    await screen.findByRole("listbox", { name: "Pages" });
  };
  const typePage = (value: string) => fireEvent.change(screen.getByRole("textbox", { name: "Page" }), { target: { value } });
  const locators = () => previews.at(-1)?.items.map((item) => item.locator_input?.text ?? item.locator?.value ?? "");
  await open("b");
  typePage("A-3");
  await waitFor(() => expect(locators()).toEqual(["", "A-3"]));
  await open("a");
  await read();
  fireEvent.click(candidate(3));
  await waitFor(() => expect(locators()).toEqual(["xii", "A-3"]));
  expect(page()).toBe("xii");
  expect(candidate(3).getAttribute("aria-selected")).toBe("true");
  expect(candidate(2).getAttribute("aria-selected")).toBe("false");
  await open("b");
  await read();
  expect(thumb("A-3").getAttribute("aria-selected")).toBe("true");
  typePage("xii");
  expect(selected().every((value) => value === "false")).toBe(true);
  fireEvent.click(candidate(2));
  await waitFor(() => expect(locators()).toEqual(["xii", "xii"]));
  await open("a");
  await read();
  expect(selected().every((value) => value === "false")).toBe(true);
  fireEvent.click(candidate(3));
  removeFromCitation(inCitation("a"));
  await waitFor(() => expect(screen.queryByRole("listbox", { name: "Pages" })).toBeNull());
  expect(inCitation("a")).toBeNull();
  await waitFor(() => expect(locators()).toEqual(["xii"]));
  await open("b");
  await read();
  expect(page()).toBe("xii");
  expect(selected().every((value) => value === "false")).toBe(true);
});

test("thumbnails choose pages as Preview selects them, and Page shows printed numbers", () => {
  render(<Harness />);
  fireEvent.click(thumb("417"));
  expect(page()).toBe("417");
  fireEvent.click(thumb("419"), { shiftKey: true });
  expect(page()).toBe("417–419");
  fireEvent.click(thumb("423"), { metaKey: true });
  expect(page()).toBe("417–419, 423");
  expect(thumb("418").getAttribute("aria-selected")).toBe("true");
});

test("typing in Page selects the same thumbnails, and scrolling alone never changes Page", () => {
  render(<Harness />);
  fireEvent.change(screen.getByRole("textbox", { name: "Page" }), { target: { value: "xi–xii, 416" } });
  expect(["xi", "xii", "416"].map((name) => thumb(name).getAttribute("aria-selected"))).toEqual(["true", "true", "true"]);
  fireEvent.scroll(screen.getByLabelText("Pages of the PDF"), { target: { scrollTop: 4000 } });
  expect(page()).toBe("xi–xii, 416");
});

test("a page with no printed number asks once, and the answer numbers only its neighbours", () => {
  render(<Harness />);
  fireEvent.click(thumb("PDF 4"));
  expect(page()).toBe("");
  const ask = screen.getByRole("textbox", { name: "What number is printed on PDF page 4?" });
  fireEvent.change(ask, { target: { value: "414" } });
  act(() => { fireEvent.click(screen.getByRole("button", { name: "Use for Nearby Pages" })); });
  expect(page()).toBe("414");
  // The plate before it is numbered too; the Roman pages are not.
  expect(thumb("413")).toBeTruthy();
  expect(thumb("xii")).toBeTruthy();
  expect(screen.queryByRole("textbox", { name: /What number/ })).toBeNull();
});

test("Cite This Page cites the page in view, and the indicator shows printed and PDF pages apart", () => {
  render(<Harness initial="416" />);
  expect(screen.getByText(/PDF page \d+ of 24/).textContent).toMatch(/^p\. .+ · PDF page \d+ of 24$/);
  fireEvent.click(screen.getByRole("button", { name: "Cite This Page" }));
  expect(page()).not.toBe("");
});
