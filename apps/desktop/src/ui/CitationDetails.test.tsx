import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { useState } from "react";
import axe from "axe-core";
import type { Locator, ReferenceForm } from "../bindings";
import { CitationDetails } from "./CitationDetails";

afterEach(cleanup);
const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");

const answer = (forms: Partial<Record<ReferenceForm, string | { reason: string }>>, current: ReferenceForm | null = "normal") => ({ current_form: current,
  forms: (["normal", "narrative", "author-only", "year-only", "full"] as const).map((form) => {
    const value = forms[form];
    return { form, example: typeof value === "string" ? value : null, reason: typeof value === "object" ? value.reason : value === undefined ? "Not in this output." : null };
  }) });
const examples = { normal: "(Example, 2026)", narrative: "Example (2026)", "year-only": "(2026)" };

function Editor({ choices = answer(examples) }: { choices?: ReturnType<typeof answer> }) {
  const [locator, setLocator] = useState<Locator | null>(null);
  const [form, setForm] = useState<ReferenceForm>("normal");
  const [prefix, setPrefix] = useState("");
  return <CitationDetails prefix={prefix} suffix="" onPrefixChange={setPrefix} onSuffixChange={vi.fn()} locator={locator} form={form}
    choices={{ ...choices, current_form: form }} onLocatorChange={setLocator} onFormChange={setForm} />;
}
const checked = () => screen.getAllByRole<HTMLInputElement>("radio").filter((radio) => radio.checked).map((radio) => radio.closest("label")?.textContent);

test("Show as offers each form this output can print as the text it would paste, and a pick changes it", () => {
  render(<Editor />);
  const group = screen.getByRole("radiogroup", { name: "Show as" });
  expect(within(group).getAllByRole("radio").map((radio) => radio.closest("label")?.textContent)).toEqual(["(Example, 2026)", "Example (2026)", "(2026)"]);
  expect(checked()).toEqual(["(Example, 2026)"]);
  fireEvent.click(screen.getByRole("radio", { name: "(2026)" }));
  expect(checked()).toEqual(["(2026)"]);
});

test("a form this output cannot print is not offered, and one quiet line says why", () => {
  render(<Editor choices={answer({ ...examples, narrative: { reason: "Cite it alone or choose another output." } })} />);
  expect(screen.queryByRole("radio", { name: "Example (2026)" })).toBeNull();
  expect(screen.getByText(/^Name in the sentence: Cite it alone or choose another output\./)).toBeTruthy();
});

test("before the engine answers, each form shows by its name and can be tried", () => {
  const change = vi.fn();
  render(<CitationDetails prefix="" suffix="" onPrefixChange={vi.fn()} onSuffixChange={vi.fn()} locator={null} form="narrative" onLocatorChange={vi.fn()} onFormChange={change} />);
  expect(checked()).toEqual(["Name in the sentence"]);
  fireEvent.click(screen.getByRole("radio", { name: "Name in brackets" }));
  expect(change).toHaveBeenCalledExactlyOnceWith("normal");
});

test("one printable form is said as a quiet line, and none says the shared reason once", () => {
  const reason = "Reference entries have no name forms. Choose another style.";
  const { rerender } = render(<Editor choices={answer({ normal: "(Example, 2026)" })} />);
  expect(screen.queryByRole("radiogroup")).toBeNull();
  expect(screen.getByText("(Example, 2026)")).toBeTruthy();
  const none = { reason };
  rerender(<CitationDetails prefix="" suffix="" onPrefixChange={vi.fn()} onSuffixChange={vi.fn()} locator={null} form="normal" onLocatorChange={vi.fn()} onFormChange={vi.fn()}
    choices={answer({ normal: none, narrative: none, "author-only": none, "year-only": none, full: none }, null)} />);
  expect(screen.queryByRole("radiogroup")).toBeNull();
  expect(screen.getAllByText(reason)).toHaveLength(1);
});

test("the locator's type is a pop-up inside the field: a new type keeps the value, and a cleared value keeps the type", async () => {
  render(<Editor />);
  fireEvent.change(screen.getByRole("textbox", { name: "Page" }), { target: { value: "4" } });
  const type = screen.getByRole("button", { name: "Locator type: Page" });
  expect(type.textContent).toBe("p.");
  fireEvent.click(type);
  fireEvent.click(await screen.findByRole("menuitemradio", { name: /^Chapter/ }));
  expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Chapter" }).value).toBe("4");
  fireEvent.change(screen.getByRole("textbox", { name: "Chapter" }), { target: { value: "" } });
  expect(screen.getByRole("button", { name: "Locator type: Chapter" })).toBeTruthy();
  expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Chapter" }).value).toBe("");
});

test("text before and after are asked for, and text already there shows from the start", () => {
  const { unmount } = render(<Editor />);
  expect(screen.queryByRole("textbox", { name: "Text before" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "+ Text before" }));
  const before = screen.getByRole("textbox", { name: "Text before" });
  expect(document.activeElement).toBe(before);
  expect(screen.queryByRole("textbox", { name: "Text after" })).toBeNull();
  unmount();
  render(<CitationDetails prefix="see" suffix="" onPrefixChange={vi.fn()} onSuffixChange={vi.fn()} locator={null} form="normal" onLocatorChange={vi.fn()} onFormChange={vi.fn()} />);
  expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Text before" }).value).toBe("see");
  expect(screen.getByRole("button", { name: "+ Text after" })).toBeTruthy();
});

test("an empty Page with the caret in it shows no placeholder", () => {
  const css = readFileSync(new FileURL("./CitationDetails.css", import.meta.url), "utf8");
  expect(css).toContain("input:focus::placeholder { color: transparent; }");
});

test.each(["macos", "windows", "linux"])("%s uses labelled shared controls and plain text", async (platform) => {
  document.documentElement.dataset.platform = platform;
  const { container } = render(<Editor />);
  const page = screen.getByRole("textbox", { name: "Page" }) as HTMLInputElement;
  expect(page.placeholder).toBe("Add page");
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
