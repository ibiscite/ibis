import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import axe from "axe-core";
import type { ReferenceForm } from "../bindings";
import { CitationDetails } from "./CitationDetails";

afterEach(cleanup);
const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");

function Editor() {
  const [page, setPage] = useState("");
  const [form, setForm] = useState<ReferenceForm>("normal");
  return <CitationDetails label="Citation details for Synthetic work" page={page} choices={{ current_form: form, forms: [
    { form: "normal", example: "(Example, 2026)", reason: null },
    { form: "narrative", example: "Example (2026)", reason: null },
    { form: "year-only", example: "(2026)", reason: null },
  ] }} onPageChange={setPage} onFormChange={setForm} />;
}

test("every native form input has the full labelled row as its pointer target", () => {
  const style = document.createElement("style");
  style.textContent = readFileSync(new FileURL("./CitationDetails.css", import.meta.url), "utf8");
  document.head.append(style);
  try {
    render(<Editor />);
    for (const radio of screen.getAllByRole("radio")) {
      const row = radio.closest("label")!;
      const wrapper = radio.parentElement!;
      expect(getComputedStyle(row).position).toBe("relative");
      expect(getComputedStyle(wrapper).display).toBe("contents");
      const target = getComputedStyle(radio);
      expect(target.position).toBe("absolute");
      expect(target.width).toBe("100%");
      expect(target.height).toBe("100%");
      expect(target.opacity).toBe("0");
    }
    const year = screen.getByRole<HTMLInputElement>("radio", { name: "Year only" });
    expect(year.tabIndex).toBe(-1);
    fireEvent.click(year.closest("label")!.querySelector(".citation-form-example")!);
    expect(year.checked).toBe(true);
    expect(screen.getByRole<HTMLInputElement>("radio", { name: "Name in brackets" }).checked).toBe(false);
  } finally { style.remove(); }
});

test("Year only uses its own engine form and unsupported choices expose the reason before editing", () => {
  const change = vi.fn();
  render(<CitationDetails label="Synthetic details" page="12" onPageChange={vi.fn()} onFormChange={change} choices={{ current_form: "normal", forms: [
    { form: "normal", example: "(Example, 2026)", reason: null },
    { form: "narrative", example: null, reason: "Cite this reference alone for Name in the sentence." },
    { form: "year-only", example: "(2026)", reason: null },
  ] }} />);
  const narrative = screen.getByRole("radio", { name: "Name in the sentence" });
  expect(narrative.matches(":disabled, [aria-disabled=true]")).toBe(true);
  expect(document.getElementById(narrative.getAttribute("aria-describedby")!)?.textContent).toContain("Cite this reference alone");
  fireEvent.click(narrative.closest("label")!.querySelector(".citation-form-example")!);
  expect(change).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("radio", { name: "Year only" }));
  expect(change).toHaveBeenCalledExactlyOnceWith("year-only");
});

test.each(["macos", "windows", "linux"])("%s uses labelled shared controls, plain text and keyboard form choices", async (platform) => {
  document.documentElement.dataset.platform = platform;
  const { container } = render(<Editor />);
  const page = screen.getByRole("textbox", { name: "Page" }) as HTMLInputElement;
  const text = "١٢–١٥ <script>";
  fireEvent.change(page, { target: { value: text } });
  expect(page.value).toBe(text);
  expect(container.querySelector("script")).toBeNull();
  expect(page.getAttribute("aria-describedby")).toBeTruthy();
  const normal = screen.getByRole("radio", { name: "Name in brackets" });
  act(() => normal.focus());
  fireEvent.keyDown(normal, { key: "ArrowDown" });
  expect(screen.getByRole("radio", { name: "Name in the sentence", checked: true })).toBeTruthy();
  const results = await axe.run(container, { rules: { "color-contrast": { enabled: false } } });
  expect(results.violations).toEqual([]);
  delete document.documentElement.dataset.platform;
});

test("fields and segmented choices use role tokens, wrap and retain keyboard and touch targets", () => {
  const css = readFileSync(new FileURL("./CitationDetails.css", import.meta.url), "utf8");
  const segmented = readFileSync(new FileURL("./Segmented.css", import.meta.url), "utf8");
  expect(css).toContain("input:focus-visible");
  expect(css).toContain("@media (pointer: coarse)");
  expect(segmented).toContain("min-height: var(--control-small-h)");
  expect(segmented).toContain("@media (pointer: coarse)");
  expect(css).not.toMatch(/#[0-9a-f]{3,8}\b|\b\d+px\b/);
});
