import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { useState } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { resetApp } from "../../testing/app";
import { Reader } from "./Reader";
import { inventedSource, type PageSource } from "./source";

afterEach(async () => { await resetApp(); vi.unstubAllGlobals(); });

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
