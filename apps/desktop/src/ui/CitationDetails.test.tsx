import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
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
/** Show as's menu, opened: each row's form name and the text it would paste. */
async function forms() {
  fireEvent.click(screen.getByRole("button", { name: /^Show as / }));
  const rows = await screen.findAllByRole("menuitemradio");
  return rows.map((row) => [row.querySelector(".menu-label")?.textContent, row.querySelector(".toolbar-menu-sample")?.textContent]);
}

test("Show as names each form this output can print, with the text it would paste, and a pick changes it", async () => {
  render(<Editor />);
  expect(screen.getByRole("button", { name: /^Show as / }).textContent).toBe("Name in brackets");
  expect(await forms()).toEqual([["Name in brackets", "(Example, 2026)"], ["Name in the sentence", "Example (2026)"], ["Year only", "(2026)"]]);
  fireEvent.click(screen.getByRole("menuitemradio", { name: "Year only" }));
  expect(screen.getByRole("button", { name: /^Show as / }).textContent).toBe("Year only");
});

test("forms this output cannot print are left out, and the menu says why once", async () => {
  render(<Editor choices={answer({ normal: "(Example, 2026)" })} />);
  expect(await forms()).toEqual([["Name in brackets", "(Example, 2026)"]]);
  expect(screen.getAllByText(/Not in this output/)).toHaveLength(1);
  expect(screen.getByText("Name in the sentence, Name only, Year only, and Full reference: Not in this output.")).toBeTruthy();
});

test("before the engine answers, every form can be tried from the pop-up by name", async () => {
  const change = vi.fn();
  render(<CitationDetails prefix="" suffix="" onPrefixChange={vi.fn()} onSuffixChange={vi.fn()} locator={null} form="narrative" onLocatorChange={vi.fn()} onFormChange={change} />);
  expect(screen.getByRole("button", { name: /^Show as / }).textContent).toBe("Name in the sentence");
  expect(await forms()).toHaveLength(5);
  fireEvent.click(screen.getByRole("menuitemradio", { name: "Name in brackets" }));
  expect(change).toHaveBeenCalledExactlyOnceWith("normal");
});

test("an output with no forms says the shared reason once, in place of the pop-up", () => {
  const reason = "Reference entries have no name forms. Choose another style.";
  const none = { reason };
  render(<CitationDetails prefix="" suffix="" onPrefixChange={vi.fn()} onSuffixChange={vi.fn()} locator={null} form="normal" onLocatorChange={vi.fn()} onFormChange={vi.fn()}
    choices={answer({ normal: none, narrative: none, "author-only": none, "year-only": none, full: none }, null)} />);
  expect(screen.queryByRole("button", { name: /^Show as / })).toBeNull();
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
  fireEvent.click(screen.getByRole("button", { name: "Text before" }));
  const before = screen.getByRole("textbox", { name: "Text before" });
  expect(document.activeElement).toBe(before);
  expect(screen.queryByRole("textbox", { name: "Text after" })).toBeNull();
  unmount();
  render(<CitationDetails prefix="see" suffix="" onPrefixChange={vi.fn()} onSuffixChange={vi.fn()} locator={null} form="normal" onLocatorChange={vi.fn()} onFormChange={vi.fn()} />);
  expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Text before" }).value).toBe("see");
  expect(screen.getByRole("button", { name: "Text after" })).toBeTruthy();
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
