import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import type { Locator, CitationForm } from "../bindings";
import { CitationDetails } from "./CitationDetails";

afterEach(cleanup);

const answer = (forms: Partial<Record<CitationForm, string | { reason: string }>>, current: CitationForm | null = "normal") => ({ current_form: current,
  forms: (["normal", "narrative", "author-only", "year-only", "full"] as const).map((form) => {
    const value = forms[form];
    return { form, example: typeof value === "string" ? value : null, reason: typeof value === "object" ? value.reason : value === undefined ? "Not in this output." : null };
  }) });
const examples = { normal: "(Example, 2026)", narrative: "Example (2026)", "year-only": "(2026)" };

function Editor({ choices = answer(examples) }: { choices?: ReturnType<typeof answer> }) {
  const [locator, setLocator] = useState<Locator | null>(null);
  const [form, setForm] = useState<CitationForm>("normal");
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
