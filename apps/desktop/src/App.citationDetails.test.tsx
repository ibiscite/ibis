import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "./App";
import { backend, prepared, receipt, resetApp, result, openCitationDetails, closeCitationDetails, type PreviewRequest } from "./testing/app";

afterEach(resetApp);

test.each(["S.12", "pages of notes"])("a form edit preserves the captured page %s as canonical text", async (value) => {
  const previews: PreviewRequest[] = [];
  const locator = { label: "page", value };
  backend({ search: () => ({ revision: "1", results: [result("a", "Synthetic work")],
    interpretation: { search: "Example", locator, typed: `pp. "${value}"` } }),
    preview_file_citation: (request) => { previews.push(request); return receipt("Canonical output"); } });
  render(<App />);
  const page = await openCitationDetails() as HTMLInputElement;
  expect(page.value).toBe(value);
  fireEvent.click(screen.getByRole("radio", { name: "Omit author" }));
  await waitFor(() => expect(previews.at(-1)?.items[0]).toEqual({
    id: result("a", "").id, cite: result("a", "").cite, locator, form: "author-omitted",
  }));
  expect(page.value).toBe(value);
});

test.each([false, true])("a label-only Page refuses copy until corrected (preview hidden: %s)", async (hidden) => {
  const message = "Reference 1: Page needs a page or range. Enter one after the label, or clear Page.";
  const copied: string[] = [];
  const calls = backend({ search: () => [result("a", "Synthetic work")],
    preview_file_citation: ({ items }) => {
      if (items[0]?.page_input === "pp.") throw { code: "failed", message };
      return receipt(items[0]?.page_input ? "Synthetic normalized output" : "Original output");
    },
    copy_file_citation: (args) => { const saved = prepared(args); copied.push(saved.preview.text); return { text: saved.preview.text, warnings: [] }; },
  });
  render(<App />);
  const page = await openCitationDetails();
  await screen.findByText("Original output");
  const search = screen.getByLabelText("Search references");
  if (hidden) fireEvent.keyDown(search, { key: "i", code: "KeyI", ctrlKey: true });
  fireEvent.change(page, { target: { value: "pp." } });
  expect((await screen.findByRole("alert")).textContent).toBe(message);
  fireEvent.keyDown(search, { key: "Enter" });
  await act(async () => { await Promise.resolve(); });
  expect(calls).not.toContain("copy_file_citation");
  expect(copied).toEqual([]);
  fireEvent.change(page, { target: { value: "pp. ١٢–١٥" } });
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  fireEvent.keyDown(search, { key: "Enter" });
  await waitFor(() => expect(copied).toEqual(["Synthetic normalized output"]));
});

test.each(["pointer", "Enter", " "])("Edit activated with %s focuses Page and edits the chosen reference", async (activation) => {
  const previews: PreviewRequest[] = [];
  const calls = backend({ search: () => [result("a", "First work"), result("b", "Second work")],
    preview_file_citation: (request) => { previews.push(request); return receipt(`preview ${previews.length}`); } });
  render(<App />);
  fireEvent.click(await screen.findByText("First work"), { ctrlKey: true });
  fireEvent.click(screen.getByText("Second work"), { ctrlKey: true });
  for (const [key, title, value] of [["b", "Second work", "29"], ["a", "First work", "12–15"], ["a", "First work", "16"]]) {
    const chip = screen.getByRole("button", { name: `Remove ${key} from the selection` }).closest("li")!;
    const edit = within(chip).getByRole("button", { name: /^Edit/ });
    act(() => edit.focus());
    if (activation === "pointer") fireEvent.click(edit);
    else {
      const code = activation === " " ? "Space" : "Enter";
      fireEvent.keyDown(edit, { key: activation, code });
      fireEvent.keyUp(edit, { key: activation, code });
    }
    const page = screen.getByRole("textbox", { name: "Page" });
    expect(document.activeElement).toBe(page);
    expect(screen.getByRole("region", { name: `Citation details for ${title}` }).contains(page)).toBe(true);
    fireEvent.change(page, { target: { value } });
    await waitFor(() => expect(previews.at(-1)?.items.find((item) => item.id.key === key)?.page_input).toBe(value));
    closeCitationDetails();
  }
  expect(previews.at(-1)?.items).toMatchObject([
    { id: result("a", "").id, locator: null, page_input: "16" },
    { id: result("b", "").id, locator: null, page_input: "29" },
  ]);
  expect(calls).not.toContain("copy_file_citation");
});

test.each([true, false])("actionable refusals remain visible with the preview hidden (hide before failure: %s)", async (hideBefore) => {
  const message = "Reference 1: Narrative cannot be expressed in this output. Choose Normal or another output.";
  const calls = backend({ search: () => [result("a", "Synthetic work")],
    preview_file_citation: ({ items }) => {
      if (items[0]?.form === "narrative") throw { code: "failed", message };
      return receipt("Synthetic output");
    } });
  render(<App />);
  await openCitationDetails();
  const search = screen.getByLabelText("Search references");
  const hide = () => fireEvent.keyDown(search, { key: "i", code: "KeyI", ctrlKey: true });
  if (hideBefore) hide();
  fireEvent.click(screen.getByRole("radio", { name: "Narrative" }));
  await screen.findByRole("alert");
  if (!hideBefore) hide();
  expect(screen.getByRole("alert").textContent).toBe(message);
  fireEvent.keyDown(search, { key: "Enter" });
  await waitFor(() => expect(screen.getAllByRole("alert").map((alert) => alert.textContent)).toEqual([message]));
  expect(calls).not.toContain("copy_file_citation");
  fireEvent.click(screen.getByRole("radio", { name: "Normal" }));
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  expect(screen.queryByRole("region", { name: "Preview" })).toBeNull();
});

test("editing the highlighted reference freezes its page and form for preview and canonical copy", async () => {
  const previews: PreviewRequest[] = [];
  const copies: PreviewRequest[] = [];
  backend({
    search: ({ query }) => query.text ? [result("b", "Second work")] : [result("a", "First work")],
    preview_file_citation: (request) => { previews.push(request); return receipt(`preview ${previews.length}`); },
    copy_file_citation: (args) => { const saved = prepared(args); copies.push(saved.request); return { text: saved.preview.text, warnings: [] }; },
  });
  render(<App />);
  const page = await openCitationDetails();
  act(() => page.focus());
  fireEvent.change(page, { target: { value: "12–15" } });
  expect(document.activeElement).toBe(page);
  fireEvent.click(screen.getByRole("radio", { name: "Narrative" }));
  await waitFor(() => expect(previews.at(-1)?.items).toMatchObject([
    { id: result("a", "").id, locator: null, page_input: "12–15", form: "narrative" },
  ]));
  const search = screen.getByLabelText("Search references");
  fireEvent.change(search, { target: { value: "second" } });
  await screen.findByText("Second work");
  expect((page as HTMLInputElement).value).toBe("12–15");
  fireEvent.keyDown(search, { key: "Enter" });
  await waitFor(() => expect(copies).toHaveLength(1));
  expect(copies[0]?.items).toMatchObject([{ id: result("a", "").id, locator: null, page_input: "12–15", form: "narrative" }]);
});

test("each shelf reference keeps independent details and an empty Page removes only that page", async () => {
  const previews: PreviewRequest[] = [];
  backend({ search: () => [result("a", "First work"), result("b", "Second work")],
    preview_file_citation: (request) => { previews.push(request); return receipt(`preview ${previews.length}`); } });
  render(<App />);
  fireEvent.click(await screen.findByText("First work"), { ctrlKey: true });
  await openCitationDetails();
  fireEvent.change(screen.getByRole("textbox", { name: "Page" }), { target: { value: "iv–vi" } });
  fireEvent.click(screen.getByRole("radio", { name: "Omit author" }));
  closeCitationDetails();
  fireEvent.click(screen.getByText("Second work"), { ctrlKey: true });
  const edit = (key: string) => within(screen.getByRole("button", { name: `Remove ${key} from the selection` }).closest("li")!)
    .getByRole("button", { name: /^Edit/ });
  fireEvent.click(edit("b"));
  expect((screen.getByRole("textbox", { name: "Page" }) as HTMLInputElement).value).toBe("");
  fireEvent.change(screen.getByRole("textbox", { name: "Page" }), { target: { value: "29" } });
  fireEvent.click(screen.getByRole("radio", { name: "Narrative" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Page" }), { target: { value: "" } });
  await waitFor(() => expect(previews.at(-1)?.items).toMatchObject([
    { id: result("a", "").id, locator: null, page_input: "iv–vi", form: "author-omitted" },
    { id: result("b", "").id, locator: null, form: "narrative" },
  ]));
  closeCitationDetails();
  fireEvent.click(edit("a"));
  expect((screen.getByRole("textbox", { name: "Page" }) as HTMLInputElement).value).toBe("iv–vi");
  expect(screen.getByRole("radio", { name: "Omit author", checked: true })).toBeTruthy();
  fireEvent.click(screen.getByRole("radio", { name: "Normal" }));
  await waitFor(() => expect(previews.at(-1)?.items[0]?.form).toBe("normal"));
});

test("losses stay beside the preview and an unsupported form blocks copying until corrected", async () => {
  const calls = backend({ search: () => [result("a", "Synthetic work")],
    preview_file_citation: ({ items }) => {
      if (items[0]?.form === "narrative") throw { code: "failed", message: "Reference 1: Narrative cannot be expressed in this output." };
      return receipt("Synthetic output", null, items[0]?.form === "author-omitted" ? ["Reference 1: Omit author cannot be fully expressed in this output."] : []);
    } });
  render(<App />);
  await openCitationDetails();
  fireEvent.click(screen.getByRole("radio", { name: "Omit author" }));
  await within(screen.getByRole("dialog")).findByText(/Reference 1: Omit author/);
  closeCitationDetails();
  const preview = screen.getByRole("region", { name: "Preview" });
  await within(preview).findByText("1 warning");
  await openCitationDetails();
  fireEvent.click(screen.getByRole("radio", { name: "Narrative" }));
  expect((await screen.findByRole("alert")).textContent).toContain("Narrative cannot be expressed");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  expect(calls).not.toContain("copy_file_citation");
  fireEvent.click(screen.getByRole("radio", { name: "Normal" }));
  await waitFor(() => expect(preview.querySelector(".preview-text")?.textContent).toBe("Synthetic output"));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await waitFor(() => expect(calls).toContain("copy_file_citation"));
});

test("a form edit cancels queued Enter and ignores the retired preview", async () => {
  const pending: ((value: ReturnType<typeof receipt>) => void)[] = [];
  const calls = backend({ search: () => [result("a", "Synthetic work")],
    preview_file_citation: () => new Promise((resolve) => pending.push(resolve)) });
  render(<App />);
  await openCitationDetails();
  await waitFor(() => expect(pending).toHaveLength(1));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  fireEvent.click(screen.getByRole("radio", { name: "Narrative" }));
  await waitFor(() => expect(pending).toHaveLength(2));
  await act(async () => { pending[1]!(receipt("New narrative")); });
  await screen.findByText("New narrative");
  await act(async () => { pending[0]!(receipt("Retired normal")); });
  expect(screen.queryByText("Retired normal")).toBeNull();
  expect(calls).not.toContain("copy_file_citation");
});

test("changing only the form does not offer to reapply an unchanged search page", async () => {
  backend({ search: () => ({ revision: "1", results: [result("a", "Synthetic work")],
    interpretation: { search: "Synthetic", locator: { label: "page", value: "12" }, language: "en-US" } }) });
  render(<App />);
  await openCitationDetails();
  fireEvent.click(screen.getByRole("radio", { name: "Narrative" }));
  closeCitationDetails();
  await screen.findByRole("button", { name: "Remove page 12 from a" });
  expect(screen.queryByRole("button", { name: "Apply locator to a" })).toBeNull();
});

test("page and form survive changing output and remain editable with the paste preview hidden", async () => {
  const previews: PreviewRequest[] = [];
  backend({ search: () => [result("a", "Synthetic work")],
    preview_file_citation: (request) => { previews.push(request); return receipt(`preview ${previews.length}`); } });
  render(<App />);
  let page = await openCitationDetails();
  fireEvent.change(page, { target: { value: "42" } });
  fireEvent.click(screen.getByRole("radio", { name: "Omit author" }));
  const search = screen.getByLabelText("Search references");
  closeCitationDetails();
  fireEvent.click(within(search.closest("header")!).getByRole("button", { name: "Copy as Pandoc key" }));
  fireEvent.click(screen.getByRole("menuitemradio", { name: "Typst key" }));
  await waitFor(() => expect(previews.at(-1)?.format).toBe("typst"));
  expect(previews.at(-1)?.items[0]).toMatchObject({ locator: null, page_input: "42", form: "author-omitted" });
  fireEvent.keyDown(search, { key: "i", code: "KeyI", ctrlKey: true });
  expect(screen.queryByRole("region", { name: "Preview" })).toBeNull();
  page = await openCitationDetails();
  fireEvent.change(page, { target: { value: "42–45" } });
  await waitFor(() => expect(previews.at(-1)?.items[0]?.page_input).toBe("42–45"));
  expect(previews.at(-1)?.items[0]?.form).toBe("author-omitted");
});
