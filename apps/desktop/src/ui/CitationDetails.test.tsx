import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
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
  return <CitationDetails page={page} form={form} choices={{ current_form: form, forms: [
    { form: "normal", example: "(Example, 2026)", reason: null },
    { form: "narrative", example: "Example (2026)", reason: null },
    { form: "year-only", example: "(2026)", reason: null },
  ] }} onPageChange={setPage} onFormChange={setForm} />;
}

test("the Form pop-up names the current form, and a pick changes it", async () => {
  render(<Editor />);
  fireEvent.click(screen.getByRole("button", { name: "Show as Name in brackets" }));
  const item = await screen.findByRole("menuitemradio", { name: "Year only" });
  expect(item.textContent).toContain("(2026)");
  fireEvent.click(item);
  expect(await screen.findByRole("button", { name: "Show as Year only" })).toBeTruthy();
});

test("Year only uses its own engine form and an unavailable form says why under its label", async () => {
  const change = vi.fn();
  render(<CitationDetails page="12" form="normal" onPageChange={vi.fn()} onFormChange={change} choices={{ current_form: "normal", forms: [
    { form: "normal", example: "(Example, 2026)", reason: null },
    { form: "narrative", example: null, reason: "Cite it alone or choose another output." },
    { form: "year-only", example: "(2026)", reason: null },
  ] }} />);
  fireEvent.click(screen.getByRole("button", { name: /^Show as / }));
  const narrative = await screen.findByRole("menuitemradio", { name: "Name in the sentence" });
  expect(narrative.getAttribute("aria-disabled")).toBe("true");
  expect(document.getElementById(narrative.getAttribute("aria-describedby")!)?.textContent).toBe("Cite it alone or choose another output.");
  fireEvent.click(narrative);
  expect(change).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("menuitemradio", { name: "Year only" }));
  expect(change).toHaveBeenCalledExactlyOnceWith("year-only");
});

test("before the engine answers the reference's own form shows and every form can be tried", async () => {
  const change = vi.fn();
  render(<CitationDetails page="" form="narrative" onPageChange={vi.fn()} onFormChange={change} />);
  fireEvent.click(screen.getByRole("button", { name: "Show as Name in the sentence" }));
  fireEvent.click(await screen.findByRole("menuitemradio", { name: "Name in brackets" }));
  expect(change).toHaveBeenCalledExactlyOnceWith("normal");
});

test.each(["macos", "windows", "linux"])("%s uses labelled shared controls and plain text", async (platform) => {
  document.documentElement.dataset.platform = platform;
  const { container } = render(<Editor />);
  const page = screen.getByRole("textbox", { name: "Page" }) as HTMLInputElement;
  const text = "١٢–١٥ <script>";
  fireEvent.change(page, { target: { value: text } });
  expect(page.value).toBe(text);
  expect(container.querySelector("script")).toBeNull();
  expect(page.getAttribute("aria-describedby")).toBeTruthy();
  const results = await axe.run(container, { rules: { "color-contrast": { enabled: false } } });
  expect(results.violations).toEqual([]);
  delete document.documentElement.dataset.platform;
});

test("the fields use role tokens and retain pointer and touch targets", () => {
  const css = readFileSync(new FileURL("./CitationDetails.css", import.meta.url), "utf8");
  expect(css).toContain("@media (pointer: coarse)");
  expect(css).toContain("min-height: var(--control-h)");
  expect(css).not.toMatch(/#[0-9a-f]{3,8}\b|\b\d+px\b/);
});
