import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { CitationDetails } from "../src/ui/CitationDetails";
import { geometry } from "./geometry";

test("form alignment measures a combined locator's fill and still reports a misplaced field", () => {
  const view = render(<CitationDetails locator={null} form="normal" prefix="see" suffix="" choices={{ current_form: "normal", forms: [
    { form: "normal", example: "(Vale, 2021)", reason: null },
    { form: "narrative", example: "Vale (2021)", reason: null },
  ] }} onLocatorChange={() => {}} onFormChange={() => {}} onPrefixChange={() => {}} onSuffixChange={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Text after" }));
  const after = screen.getByRole<HTMLInputElement>("textbox", { name: "Text after" });
  let misplaced: Element | null = null;
  for (const input of view.container.querySelectorAll("input")) input.style.border = "4px solid transparent";
  const boxes = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    // The type selector and value are deliberately narrower than their shared fill.
    const inner = this.closest(".citation-locator");
    if (inner && this !== inner) return new DOMRect(this.matches("input") ? 150 : 100, 0, 50, 24);
    const input = this.matches("input");
    return new DOMRect((input ? 96 : 100) + (this === misplaced ? 8 : 0), 0, input ? 208 : 200, 24);
  });
  const alignment = () => geometry(view.container).filter((failure) => failure.startsWith("[C19]"));
  try {
    expect(alignment()).toEqual([]);
    misplaced = view.container.querySelector(".citation-locator");
    expect(alignment()).toEqual(expect.arrayContaining([expect.stringContaining("div.citation-locator: start 108.0, the form's 100.0")]));
    misplaced = after;
    expect(alignment().some((failure) => failure.startsWith("[C19] input.") && failure.includes("start 108.0, the form's 100.0"))).toBe(true);
  } finally { boxes.mockRestore(); view.unmount(); }
});
