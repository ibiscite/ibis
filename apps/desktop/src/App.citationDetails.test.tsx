import { afterEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import { backend, chooseForm, prepared, receipt, resetApp, result, openCitationDetails, closeCitationDetails, type PreviewRequest } from "./testing/app";

afterEach(resetApp);

test.each([96, 280].flatMap((height) => [false, true].flatMap((built) =>
  ["observer", "window"].map((reflow) => ({ height, built, reflow })),
)))("a delayed long refusal keeps priority over automatic row scrolling (%j)", async ({ height, built, reflow }) => {
  const message = "Reference 1: Choose another form or clear Page. ".repeat(30);
  const observations: { callback: () => void; nodes: Set<Element> }[] = [];
  vi.stubGlobal("ResizeObserver", class {
    nodes = new Set<Element>(); constructor(callback: () => void) { observations.push({ callback, nodes: this.nodes }); }
    observe(node: Element) { this.nodes.add(node); } unobserve(node: Element) { this.nodes.delete(node); } disconnect() { this.nodes.clear(); }
  });
  let rejectPreview: ((reason: unknown) => void) | undefined;
  const calls = backend({ search: () => [result("a", "Invented work")],
    preview_file_citation: () => new Promise((_, reject) => { rejectPreview = reject; }),
  });
  const computed = window.getComputedStyle;
  const styles = vi.spyOn(window, "getComputedStyle").mockImplementation((node) => {
    if (!node.matches(".results-area, .picker-content")) return computed(node);
    const style = document.createElement("div").style;
    style.overflowY = node.matches(".picker-content") ? "auto" : "visible";
    return style;
  });
  const boxes = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const middle = this.closest(".picker-content");
    const hasRefusal = Boolean(middle?.querySelector(".citation-problem"));
    if (this.matches(".picker-content")) return new DOMRect(0, 80, 480, height);
    if (this.matches(".notices")) return new DOMRect(0, 80 - middle!.scrollTop, 480, hasRefusal ? 1200 : 0);
    if (this.matches(".citation-problem")) return new DOMRect(24, 92 - middle!.scrollTop, 432, 1176);
    if (this.matches('[role="option"], .result-title')) return new DOMRect(8, 116 + (hasRefusal ? 1200 : 0) - middle!.scrollTop, 464, this.matches(".result-title") ? 776 : 800);
    return new DOMRect();
  });
  const heights = vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this.matches(".picker-content") ? height : 0;
  });
  let view: ReturnType<typeof render> | undefined;
  try {
    view = render(<App />);
    await screen.findByText("Invented work");
    await waitFor(() => expect(rejectPreview).toBeTypeOf("function"));
    const search = screen.getByLabelText("Search references");
    const middle = document.querySelector<HTMLElement>(".picker-content")!;
    const row = screen.getByRole("option");
    // jsdom has no layout/native scrolling. Model nearest scrolling of the
    // probe's tall title so the selected-preview effect can also lose a notice.
    Object.defineProperty(row, "scrollIntoView", { value: () => {
      const item = row.getBoundingClientRect(), port = middle.getBoundingClientRect();
      if (item.top >= port.bottom || item.bottom <= port.top) middle.scrollTop += item.top - port.top;
    } });
    if (built) fireEvent.keyDown(search, { key: "Enter", shiftKey: true });
    act(() => search.focus()); middle.scrollTop = 380; fireEvent.scroll(middle);
    await act(async () => rejectPreview!({ code: "failed", message }));
    const refusal = await screen.findByRole("alert");
    expect(refusal.textContent).toBe(message);
    expect(refusal.getBoundingClientRect().top).toBe(middle.getBoundingClientRect().top);
    expect(refusal.tabIndex).toBe(0);
    expect(document.activeElement).toBe(search);
    expect(screen.queryByRole("dialog")).toBeNull();
    // ResizeObserver runs after React's commit, including its first delivery.
    // The previous regression stopped before these App observers could scroll
    // the highlighted result past the refusal. Native resize has the same risk.
    act(() => {
      if (reflow === "window") fireEvent.resize(window);
      else for (const observation of observations) if (observation.nodes.has(middle)) observation.callback();
    });
    expect(refusal.getBoundingClientRect().top).toBe(middle.getBoundingClientRect().top);
    expect(document.activeElement).toBe(search);
    middle.scrollTop += 240; fireEvent.scroll(middle);
    const readingTop = refusal.getBoundingClientRect().top;
    act(() => {
      if (reflow === "window") fireEvent.resize(window);
      else for (const observation of observations) if (observation.nodes.has(middle)) observation.callback();
    });
    expect(refusal.getBoundingClientRect().top).toBe(readingTop);
    fireEvent.keyDown(search, { key: "Enter" });
    expect(calls).not.toContain("copy_file_citation");
    expect(refusal.textContent).toBe(message);
    expect(refusal.getBoundingClientRect().top).toBe(readingTop);
  } finally { view?.unmount(); boxes.mockRestore(); styles.mockRestore(); heights.mockRestore(); vi.unstubAllGlobals(); }
});

test("explicit arrows still reveal another result while a built citation is refused", async () => {
  const message = "Reference 1: Choose another form or clear Page.";
  backend({ search: () => [result("a", "First invented work"), result("b", "Second invented work")],
    preview_file_citation: () => { throw { code: "failed", message }; },
  });
  render(<App />);
  const refusal = await screen.findByRole("alert");
  const search = screen.getByLabelText("Search references");
  fireEvent.keyDown(search, { key: "Enter", shiftKey: true });
  const next = screen.getByRole("option", { name: /Second invented work/ });
  const reveal = vi.fn(); Object.defineProperty(next, "scrollIntoView", { value: reveal });
  fireEvent.keyDown(search, { key: "ArrowDown" });
  expect(next.getAttribute("aria-selected")).toBe("true");
  expect(reveal).toHaveBeenCalledExactlyOnceWith({ block: "nearest" });
  expect(refusal.textContent).toBe(message);
  expect(document.activeElement).toBe(search);
});

test("removing a middle reference and Undo restore its position and page through the real picker", async () => {
  backend({ search: () => [result("a", "First work"), result("b", "Second work"), result("c", "Third work")] });
  render(<App />);
  fireEvent.click(await screen.findByText("First work"), { ctrlKey: true });
  fireEvent.click(screen.getByText("Second work"), { ctrlKey: true });
  fireEvent.click(screen.getByText("Third work"), { ctrlKey: true });
  const openReferences = async () => {
    fireEvent.click(screen.getByRole("button", { name: "3 references" }));
    return screen.findByRole("menu");
  };
  let list = await openReferences();
  const names = () => within(list).getAllByRole("menuitemradio").map((item) => item.querySelector(".menu-label")?.textContent);
  const order = names();
  fireEvent.click(within(list).getByRole("menuitemradio", { name: "Smith 2020 · b" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Page" }), { target: { value: "iv–vi" } });
  fireEvent.click(screen.getByRole("button", { name: "Remove from Citation" }));
  expect(screen.queryByRole("region", { name: /^Citation details for/ })).toBeNull();
  expect(screen.getByRole("button", { name: "2 references" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: /^Undo / }));
  list = await openReferences();
  expect(names()).toEqual(order);
  fireEvent.click(within(list).getByRole("menuitemradio", { name: "Smith 2020 · b" }));
  expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Page" }).value).toBe("iv–vi");
});

test("closing and reopening while Page has focus returns to Search and retains the edited reference", async () => {
  backend({ search: () => [result("a", "Invented work")] });
  render(<App />);
  const page = await openCitationDetails();
  fireEvent.change(page, { target: { value: "42" } });
  await act(async () => { await emit("picker:hidden"); await emit("picker:shown"); await new Promise(requestAnimationFrame); });
  expect(document.activeElement).toBe(screen.getByLabelText("Search references"));
  expect(screen.queryByRole("region", { name: /^Citation details for/ })).toBeNull();
  expect((await openCitationDetails()).value).toBe("42");
});

test("reference and window Undo keep separate reachable actions and cannot consume each other", async () => {
  const windowUndos: unknown[] = [];
  backend({ search: () => [result("a", "Invented work")], undo_window_position: (args) => { windowUndos.push(args); return true; } });
  render(<App />);
  const search = screen.getByLabelText("Search references");
  await screen.findByText("Invented work");
  fireEvent.keyDown(search, { key: "Enter", shiftKey: true });
  const segment = screen.getByRole("button", { name: "Edit Smith 2020" });
  act(() => segment.focus()); fireEvent.keyDown(segment, { key: "Backspace" });
  await act(async () => { await emit("picker:window-reset", { revision: 17, undo: 17 }); });
  const actions = screen.getAllByRole("button", { name: /^Undo / });
  expect(actions).toHaveLength(2);
  const descriptions = actions.map((button) => document.getElementById(button.getAttribute("aria-describedby")!)?.textContent);
  expect(descriptions).toEqual(["Removed Smith, 2020 from citation.", "Window position reset."]);
  for (const button of actions) { act(() => button.focus()); expect(document.activeElement).toBe(button); }
  await act(async () => { fireEvent.click(actions[1]!); });
  expect(windowUndos).toEqual([{ revision: 17 }]);
  expect(screen.queryByText("Window position reset.")).toBeNull();
  expect(screen.getByText("Removed Smith, 2020 from citation.")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: /^Undo / }));
  expect(screen.queryByRole("button", { name: /^Undo / })).toBeNull();
  expect(screen.getByRole("button", { name: "Remove a from the selection" })).toBeTruthy();
});

test("source and sentence-case warnings stay visible in preview and copy feedback", async () => {
  const warnings = [
    "Reference 1: A source field could not be fully converted. Check the library entry.",
    "Reference 1: APA expects sentence case for this title. Stored capitalization was preserved; check it in your document.",
  ];
  const calls = backend({ search: () => [result("protected2020", "Using iPhone for NASA")],
    preview_file_citation: () => receipt("Okafor, A. (2020). Using iPhone for NASA. Invented Journal.", null, warnings),
    copy_file_citation: (args) => { const saved = prepared(args); return { text: saved.preview.text, warnings: saved.preview.warnings }; },
  });
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: "2 warnings" }));
  const details = await screen.findByRole("region", { name: "Citation warnings" });
  for (const warning of warnings) expect(within(details).getByText(warning)).toBeTruthy();
  fireEvent.keyDown(details, { key: "Escape" });
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  const feedback = await screen.findByRole("list", { name: "Copy warnings" });
  for (const warning of warnings) expect(within(feedback).getByText(warning)).toBeTruthy();
  expect(calls).not.toContain("hide_picker");
  expect(calls).not.toContain("show_copied_notice");
});

test.each(["S.12", "pages of notes"])("a form edit preserves the captured page %s as canonical text", async (value) => {
  const previews: PreviewRequest[] = [];
  const locator = { label: "page", value };
  backend({ search: () => ({ revision: "1", results: [result("a", "Synthetic work")],
    interpretation: { search: "Example", locator, typed: `pp. "${value}"` } }),
    preview_file_citation: (request) => { previews.push(request); return receipt("Canonical output"); } });
  render(<App />);
  const page = await openCitationDetails() as HTMLInputElement;
  expect(page.value).toBe(value);
  await chooseForm("Name in the sentence");
  await waitFor(() => expect(previews.at(-1)?.items[0]).toEqual({
    id: result("a", "").id, cite: result("a", "").cite, locator, form: "narrative",
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
  await within(screen.getByRole("region", { name: "Full citation" })).findByText("Original output");
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
    fireEvent.click(screen.getByRole("button", { name: "2 references" }));
    const edit = within(await screen.findByRole("menu")).getByRole("menuitemradio", { name: `Smith 2020 · ${key}` });
    if (activation === "pointer") fireEvent.click(edit);
    else {
      const code = activation === " " ? "Space" : "Enter";
      act(() => edit.focus());
      fireEvent.keyDown(edit, { key: activation, code });
      fireEvent.keyUp(edit, { key: activation, code });
    }
    const page = await screen.findByRole("textbox", { name: "Page" });
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
  await chooseForm("Name in the sentence");
  await screen.findByRole("alert");
  if (!hideBefore) hide();
  expect(screen.getByRole("alert").textContent).toBe(message);
  fireEvent.keyDown(search, { key: "Enter" });
  await waitFor(() => expect(screen.getAllByRole("alert").map((alert) => alert.textContent)).toEqual([message]));
  expect(calls).not.toContain("copy_file_citation");
  await chooseForm("Name in brackets");
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
  await chooseForm("Name in the sentence");
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
  await chooseForm("Name in the sentence");
  closeCitationDetails();
  fireEvent.click(screen.getByText("Second work"), { ctrlKey: true });
  const edit = async (key: string) => {
    fireEvent.click(screen.getByRole("button", { name: "2 references" }));
    return within(await screen.findByRole("menu")).getByRole("menuitemradio", { name: `Smith 2020 · ${key}` });
  };
  fireEvent.click(await edit("b"));
  expect((screen.getByRole("textbox", { name: "Page" }) as HTMLInputElement).value).toBe("");
  fireEvent.change(screen.getByRole("textbox", { name: "Page" }), { target: { value: "29" } });
  await chooseForm("Name in the sentence");
  fireEvent.change(screen.getByRole("textbox", { name: "Page" }), { target: { value: "" } });
  await waitFor(() => expect(previews.at(-1)?.items).toMatchObject([
    { id: result("a", "").id, locator: null, page_input: "iv–vi", form: "narrative" },
    { id: result("b", "").id, locator: null, form: "narrative" },
  ]));
  closeCitationDetails();
  fireEvent.click(await edit("a"));
  expect((screen.getByRole("textbox", { name: "Page" }) as HTMLInputElement).value).toBe("iv–vi");
  expect(screen.getByRole("button", { name: "Show as Name in the sentence" })).toBeTruthy();
  await chooseForm("Name in brackets");
  await waitFor(() => expect(previews.at(-1)?.items[0]?.form).toBe("normal"));
});

test("losses keep a visible count and an unsupported form blocks copying until corrected", async () => {
  const warning = "Reference 1: Page is not included in this output.";
  const calls = backend({ search: () => [result("a", "Synthetic work")],
    preview_file_citation: ({ items }) => {
      if (items[0]?.form === "narrative") throw { code: "failed", message: "Reference 1: Narrative cannot be expressed in this output." };
      return receipt("Synthetic output", null, items[0]?.page_input ? [warning] : []);
    } });
  render(<App />);
  const page = await openCitationDetails();
  fireEvent.change(page, { target: { value: "12" } });
  await within(screen.getByRole("region", { name: /^Citation details for/ })).findByText(warning);
  closeCitationDetails();
  const preview = screen.getByRole("region", { name: "Preview" });
  const warnings = await within(preview).findByRole("button", { name: "1 warning" });
  expect(warnings.textContent).toBe("1 warning");
  fireEvent.click(warnings);
  const notes = screen.getByRole("region", { name: "Citation warnings" });
  expect(within(notes).getByText(warning)).toBeTruthy();
  fireEvent.keyDown(notes, { key: "Escape" });
  await openCitationDetails();
  await chooseForm("Name in the sentence");
  expect((await screen.findByRole("alert")).textContent).toContain("Narrative cannot be expressed");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  expect(calls).not.toContain("copy_file_citation");
  await chooseForm("Name in brackets");
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
  await chooseForm("Name in the sentence");
  await waitFor(() => expect(pending).toHaveLength(2));
  await act(async () => { pending[1]!(receipt("New narrative")); });
  await within(screen.getByRole("region", { name: "Full citation" })).findByText("New narrative");
  await act(async () => { pending[0]!(receipt("Retired normal")); });
  expect(screen.queryByText("Retired normal")).toBeNull();
  expect(calls).not.toContain("copy_file_citation");
});

test("changing only the form does not offer to reapply an unchanged search page", async () => {
  backend({ search: () => ({ revision: "1", results: [result("a", "Synthetic work")],
    interpretation: { search: "Synthetic", locator: { label: "page", value: "12" }, language: "en-US" } }) });
  render(<App />);
  await openCitationDetails();
  await chooseForm("Name in the sentence");
  closeCitationDetails();
  const reopenedPage = await openCitationDetails();
  expect(reopenedPage.value).toBe("12");
  closeCitationDetails();
  expect(screen.queryByRole("button", { name: "Apply locator to a" })).toBeNull();
});

test("page and form survive changing output and remain editable with the paste preview hidden", async () => {
  const previews: PreviewRequest[] = [];
  backend({ search: () => [result("a", "Synthetic work")],
    preview_file_citation: (request) => { previews.push(request); return receipt(`preview ${previews.length}`); } });
  render(<App />);
  let page = await openCitationDetails();
  fireEvent.change(page, { target: { value: "42" } });
  await chooseForm("Name in the sentence");
  const search = screen.getByLabelText("Search references");
  closeCitationDetails();
  fireEvent.click(within(search.closest("header")!).getByRole("button", { name: "Copy as Pandoc key" }));
  fireEvent.click(screen.getByRole("menuitemradio", { name: "Typst key" }));
  await waitFor(() => expect(previews.at(-1)?.format).toBe("typst"));
  expect(previews.at(-1)?.items[0]).toMatchObject({ locator: null, page_input: "42", form: "narrative" });
  fireEvent.keyDown(search, { key: "i", code: "KeyI", ctrlKey: true });
  expect(screen.queryByRole("region", { name: "Preview" })).toBeNull();
  page = await openCitationDetails();
  fireEvent.change(page, { target: { value: "42–45" } });
  await waitFor(() => expect(previews.at(-1)?.items[0]?.page_input).toBe("42–45"));
  expect(previews.at(-1)?.items[0]?.form).toBe("narrative");
});

test("late form metadata cannot overwrite the examples for a newer Page edit or reopened menu", async () => {
  const replies: ((value: unknown) => void)[] = [];
  const requests: { ticket: string; item: number }[] = [];
  backend({ search: () => [result("a", "Synthetic metadata work")],
    preview_file_citation: ({ items }) => receipt(`Page ${items[0]?.page_input ?? "original"}`),
    citation_details: (args) => { requests.push({ ticket: args.ticket, item: Number(args.item) }); return new Promise((resolve) => replies.push(resolve)); },
  });
  render(<App />);
  const page = await openCitationDetails();
  await waitFor(() => expect(replies).toHaveLength(1));
  fireEvent.change(page, { target: { value: "9" } });
  await waitFor(() => expect(replies).toHaveLength(2));
  expect(requests[1]?.ticket).not.toBe(requests[0]?.ticket);
  const choices = (example: string) => ({ current_form: "normal", forms: [
    { form: "normal", example, reason: null }, { form: "narrative", example: null, reason: "Cite it alone." }, { form: "year-only", example: "(2026)", reason: null },
  ] });
  await act(async () => replies[1]!(choices("Current page 9 example")));
  await act(async () => replies[0]!(choices("Retired original example")));
  const form = screen.getByRole("button", { name: /^Show as / });
  fireEvent.click(form);
  expect(await screen.findByText("Current page 9 example")).toBeTruthy();
  expect(screen.queryByText("Retired original example")).toBeNull();
  fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
  closeCitationDetails();
  await openCitationDetails();
  await waitFor(() => expect(replies).toHaveLength(3));
  await act(async () => replies[2]!(choices("Reopened page 9 example")));
  expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Page" }).value).toBe("9");
  fireEvent.click(screen.getByRole("button", { name: /^Show as / }));
  expect(await screen.findByText("Reopened page 9 example")).toBeTruthy();
});

test("true Year only is sent to the engine; an engine refusal can be corrected from the inspector", async () => {
  const previews: PreviewRequest[] = [];
  backend({ search: () => [result("a", "Synthetic year work")],
    preview_file_citation: (request) => {
      previews.push(request);
      return request.items[0]?.form === "year-only"
        ? { ...receipt(""), spans: [], problem: "Year only cannot show this reference’s year in this output." }
        : receipt("(Smith, 2020)");
    },
  });
  render(<App />);
  await openCitationDetails();
  await chooseForm("Year only");
  expect((await screen.findByRole("alert")).textContent).toContain("Year only cannot show");
  expect(previews.at(-1)?.items[0]?.form).toBe("year-only");
  await chooseForm("Name in brackets");
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  expect(previews.at(-1)?.items[0]?.form).toBe("normal");
  fireEvent.click(screen.getByRole("button", { name: /^Show as / }));
  expect(await screen.findByText("(Smith, 2020)", { selector: ".toolbar-menu-sample" })).toBeTruthy();
});

test("the panel opens beside a live list; Esc steps back one layer and returns to the segment", async () => {
  backend({ search: () => ({ revision: "1", results: [result("a", "Invented work")] }), hide_picker: () => true });
  render(<App />);
  const search = screen.getByLabelText("Search references");
  await screen.findByText("Invented work");
  fireEvent.keyDown(search, { key: "Enter", shiftKey: true });
  const segment = await screen.findByRole("button", { name: "Edit Smith 2020" });
  act(() => segment.focus());
  fireEvent.keyDown(segment, { key: "Enter", code: "Enter" });
  fireEvent.keyUp(segment, { key: "Enter", code: "Enter" });
  const page = await screen.findByRole("textbox", { name: "Page" });
  expect(document.activeElement).toBe(page);
  expect(screen.getByRole("option")).toBeTruthy();
  expect(segment.getAttribute("aria-expanded")).toBe("true");
  expect(document.getElementById(segment.getAttribute("aria-controls")!)?.contains(page)).toBe(true);
  expect(screen.queryByRole("dialog")).toBeNull();
  fireEvent.keyDown(page, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("textbox", { name: "Page" })).toBeNull());
  expect(document.activeElement).toBe(segment);
  // React Aria saw the focus, so the keyboard ring draws.
  expect(segment.hasAttribute("data-focus-visible")).toBe(true);
  expect(segment.getAttribute("aria-expanded")).toBe("false");
});

test("Search and the list stay live beside the panel, and Esc from Search closes the panel first", async () => {
  const calls = backend({ search: () => [result("a", "First work"), result("b", "Second work")], hide_picker: () => true });
  render(<App />);
  fireEvent.click(await screen.findByText("First work"), { ctrlKey: true });
  await openCitationDetails();
  const search = screen.getByLabelText("Search references");
  act(() => search.focus());
  expect(screen.getByRole("textbox", { name: "Page" })).toBeTruthy();
  fireEvent.keyDown(search, { key: "ArrowDown" });
  await waitFor(() => expect(screen.getByRole("option", { name: /Second work/ }).getAttribute("aria-selected")).toBe("true"));
  expect(screen.getByRole("region", { name: "Citation details for First work" })).toBeTruthy();
  fireEvent.keyDown(search, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("textbox", { name: "Page" })).toBeNull());
  expect(document.activeElement).toBe(search);
  expect(calls).not.toContain("hide_picker");
});

test("a citation refusal shows once, in the panel beside its fields, and returns to the list when it closes", async () => {
  const message = "Reference 1: Page needs a page or range.";
  backend({ search: () => [result("a", "Synthetic work")], preview_file_citation: () => { throw { code: "failed", message }; } });
  render(<App />);
  const inList = await screen.findByRole("alert");
  expect(inList.closest(".notices")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Details" }));
  const alerts = await screen.findAllByRole("alert");
  expect(alerts.map((alert) => alert.textContent)).toEqual([message]);
  expect(alerts[0]!.closest(".inspector")).toBeTruthy();
  closeCitationDetails();
  await waitFor(() => expect(document.querySelector(".inspector")).toBeNull());
  expect((await screen.findByRole("alert")).closest(".notices")).toBeTruthy();
});

test("Esc on a focused segment reaches the picker", async () => {
  const calls = backend({ search: () => ({ revision: "1", results: [result("a", "Invented work")] }), hide_picker: () => true });
  render(<App />);
  await screen.findByText("Invented work");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter", shiftKey: true });
  const segment = await screen.findByRole("button", { name: "Edit Smith 2020" });
  act(() => segment.focus());
  fireEvent.keyDown(segment, { key: "Escape" });
  await waitFor(() => expect(calls).toContain("hide_picker"));
});

test("with the inspector open, Esc from anywhere closes it before the picker steps back", async () => {
  const calls = backend({ search: () => ({ revision: "1", results: [result("a", "Invented work")] }), hide_picker: () => true });
  render(<App />);
  await openCitationDetails();
  const more = screen.getByRole("button", { name: /^More/ });
  act(() => more.focus());
  fireEvent.keyDown(more, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("textbox", { name: "Page" })).toBeNull());
  expect(calls).not.toContain("hide_picker");
});

test("a Form pick keeps focus in the inspector, so the next key stays there", async () => {
  backend({ search: () => [result("a", "Invented work")] });
  render(<App />);
  await openCitationDetails();
  await chooseForm("Year only");
  const form = await screen.findByRole("button", { name: "Show as Year only" });
  expect(document.activeElement).toBe(form);
  fireEvent.keyDown(form, { key: "a" });
  expect(screen.getByRole("textbox", { name: "Page" })).toBeTruthy();
});

test("Esc closes the Form menu first, then the inspector", async () => {
  backend({ search: () => [result("a", "Invented work")] });
  render(<App />);
  const page = await openCitationDetails();
  fireEvent.click(screen.getByRole("button", { name: /^Show as / }));
  const menu = await screen.findByRole("menu");
  fireEvent.keyDown(menu, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
  expect(page.isConnected).toBe(true);
  fireEvent.keyDown(page, { key: "Escape" });
  await waitFor(() => expect(page.isConnected).toBe(false));
});

test("Enter in Page does what the footer says, once, and a held key does not repeat it", async () => {
  const copied: string[] = [];
  backend({ search: () => [result("a", "Invented work")], preview_file_citation: ({ items }) => receipt(`Citation ${items[0]?.page_input ?? ""}`.trim()),
    copy_file_citation: (args) => { const saved = prepared(args); copied.push(saved.preview.text); return { text: saved.preview.text, warnings: [] }; } });
  render(<App />);
  const page = await openCitationDetails();
  fireEvent.change(page, { target: { value: "42" } });
  await within(screen.getByRole("region", { name: "Full citation" })).findByText("Citation 42");
  fireEvent.keyDown(page, { key: "Enter", repeat: true });
  fireEvent.keyDown(page, { key: "Enter", isComposing: true });
  expect(copied).toEqual([]);
  fireEvent.keyDown(page, { key: "Enter" });
  await waitFor(() => expect(copied).toEqual(["Citation 42"]));
});

test("a refused page stays visible in place, and Enter in Page copies nothing", async () => {
  const message = "Reference 1: Page needs a page or range.";
  const calls = backend({ search: () => [result("a", "Synthetic work")], preview_file_citation: ({ items }) => {
    if (items[0]?.page_input === "pp.") throw { code: "failed", message };
    return receipt("Original output");
  } });
  render(<App />);
  const page = await openCitationDetails();
  fireEvent.change(page, { target: { value: "pp." } });
  const refusal = await within(screen.getByRole("region", { name: /^Citation details for/ })).findByRole("alert");
  expect(refusal.textContent).toBe(message);
  expect(document.activeElement).toBe(page);
  fireEvent.keyDown(page, { key: "Enter" });
  await act(async () => { await Promise.resolve(); });
  expect(calls).not.toContain("copy_file_citation");
  expect(document.activeElement).toBe(page);
  expect(within(screen.getByRole("region", { name: /^Citation details for/ })).getByRole("alert").textContent).toBe(message);
});

test("failed examples leave every form usable and say so once, quietly", async () => {
  const previews: PreviewRequest[] = [];
  backend({ search: () => [result("a", "Invented work")], citation_details: () => { throw { code: "failed", message: "Invented failure" }; },
    preview_file_citation: (request) => { previews.push(request); return receipt("Output"); } });
  render(<App />);
  await openCitationDetails();
  fireEvent.click(screen.getByRole("button", { name: /^Show as / }));
  expect(await screen.findByText("Examples are unavailable")).toBeTruthy();
  fireEvent.click(screen.getByRole("menuitemradio", { name: "Year only" }));
  await waitFor(() => expect(previews.at(-1)?.items[0]?.form).toBe("year-only"));
  expect(screen.queryByRole("alert")).toBeNull();
});

test("an unavailable form is dimmed with its reason under it and cannot be picked", async () => {
  const previews: PreviewRequest[] = [];
  const reason = "Cite it alone or choose another output.";
  backend({ search: () => [result("a", "Invented work")], preview_file_citation: (request) => { previews.push(request); return receipt("Output"); },
    citation_details: () => ({ current_form: "normal", forms: [{ form: "normal", example: "(Smith, 2020)", reason: null }, { form: "narrative", example: null, reason }, { form: "year-only", example: "(2020)", reason: null }] }) });
  render(<App />);
  await openCitationDetails();
  expect(screen.queryByText(reason)).toBeNull();
  await act(async () => { await Promise.resolve(); });
  fireEvent.click(screen.getByRole("button", { name: /^Show as / }));
  const item = await screen.findByRole("menuitemradio", { name: "Name in the sentence" });
  expect(item.getAttribute("aria-disabled")).toBe("true");
  expect(document.getElementById(item.getAttribute("aria-describedby")!)?.textContent).toBe(reason);
  fireEvent.click(item);
  expect(previews.every((request) => request.items[0]?.form !== "narrative")).toBe(true);
});

test("examples from the last answer stay while a page is typed, so the Form menu never empties", async () => {
  const replies: ((value: unknown) => void)[] = [];
  backend({ search: () => [result("a", "Invented work")], preview_file_citation: ({ items }) => receipt(`Page ${items[0]?.page_input ?? ""}`),
    citation_details: () => new Promise((resolve) => replies.push(resolve)) });
  render(<App />);
  const page = await openCitationDetails();
  await waitFor(() => expect(replies).toHaveLength(1));
  await act(async () => replies[0]!({ current_form: "normal", forms: [{ form: "normal", example: "First example", reason: null }, { form: "narrative", example: null, reason: null }, { form: "year-only", example: null, reason: null }] }));
  fireEvent.change(page, { target: { value: "9" } });
  await waitFor(() => expect(replies).toHaveLength(2));
  fireEvent.click(screen.getByRole("button", { name: /^Show as / }));
  expect(await screen.findByText("First example")).toBeTruthy();
});

test("pressing another segment moves the inspector to it and retires the first reference's examples", async () => {
  const replies: { item: number; resolve: (value: unknown) => void }[] = [];
  backend({ search: () => [result("a", "First work"), result("b", "Second work")],
    preview_file_citation: (request) => ({ ...receipt("(First; Second)"), spans: [{ item: 0, text: "First" }, { item: null, text: "; " }, { item: 1, text: "Second" }].slice(0, request.items.length * 2 - 1) }),
    citation_details: (args) => new Promise((resolve) => replies.push({ item: Number(args.item), resolve })) });
  render(<App />);
  fireEvent.click(await screen.findByText("First work"), { ctrlKey: true });
  fireEvent.click(screen.getByText("Second work"), { ctrlKey: true });
  fireEvent.click(await screen.findByRole("button", { name: "Edit Smith 2020 · a" }));
  await screen.findByRole("region", { name: "Citation details for First work" });
  await waitFor(() => expect(replies.at(-1)?.item).toBe(0));
  const first = replies.at(-1)!;
  fireEvent.click(screen.getByRole("button", { name: "Edit Smith 2020 · b" }));
  await screen.findByRole("region", { name: "Citation details for Second work" });
  await waitFor(() => expect(replies.at(-1)?.item).toBe(1));
  const choices = (example: string) => ({ current_form: "normal", forms: [{ form: "normal", example, reason: null }, { form: "narrative", example: null, reason: null }, { form: "year-only", example: null, reason: null }] });
  await act(async () => replies.at(-1)!.resolve(choices("Second example")));
  await act(async () => first.resolve(choices("Retired first example")));
  fireEvent.click(screen.getByRole("button", { name: /^Show as / }));
  expect(await screen.findByText("Second example")).toBeTruthy();
  expect(screen.queryByText("Retired first example")).toBeNull();
  expect(document.querySelector(".inspector")).toBeTruthy();
});

test("a form picked while no receipt exists shows the form asked for, and an old reason cannot block it", async () => {
  const previews: PreviewRequest[] = [];
  backend({ search: () => [result("a", "Synthetic work")],
    citation_details: () => ({ current_form: "normal", forms: [{ form: "normal", example: "(Smith, 2020)", reason: null }, { form: "narrative", example: null, reason: "Cite it alone." }, { form: "year-only", example: "(2020)", reason: null }] }),
    preview_file_citation: (request) => {
      previews.push(request);
      if (request.items[0]?.page_input === "pp.") throw { code: "failed", message: "Reference 1: Page needs a page or range." };
      return receipt("(Smith, 2020)");
    } });
  render(<App />);
  const page = await openCitationDetails();
  await act(async () => { await Promise.resolve(); });
  fireEvent.change(page, { target: { value: "pp." } });
  await screen.findByRole("alert");
  await chooseForm("Name in the sentence");
  expect(await screen.findByRole("button", { name: "Show as Name in the sentence" })).toBeTruthy();
  await waitFor(() => expect(previews.at(-1)?.items[0]?.form).toBe("narrative"));
});

test("a copy that fails says so in the panel, once, so it is seen when the panel replaces the list", async () => {
  backend({ search: () => [result("a", "Invented work")], preview_file_citation: () => receipt("Citation"), copy_file_citation: () => { throw new Error("SYNTHETIC_PRIVATE_PAYLOAD"); } });
  render(<App />);
  const page = await openCitationDetails();
  await within(screen.getByRole("region", { name: "Full citation" })).findByText("Citation");
  fireEvent.keyDown(page, { key: "Enter" });
  const alert = await screen.findByRole("alert");
  expect(alert.closest(".inspector")).toBeTruthy();
  expect(alert.textContent).not.toContain("SYNTHETIC_PRIVATE_PAYLOAD");
  expect(screen.getByRole("region", { name: "Full citation" }).textContent).toBe("Citation");
  closeCitationDetails();
  await waitFor(() => expect(document.querySelector(".inspector")).toBeNull());
  expect((await screen.findByRole("alert")).closest(".notices")).toBeTruthy();
});

test.each([1, 2])("removing the edited reference closes the panel and returns to Search (%s chosen)", async (count) => {
  backend({ search: () => [result("a", "First work"), result("b", "Second work")] });
  render(<App />);
  fireEvent.click(await screen.findByText("First work"), { ctrlKey: true });
  if (count === 2) fireEvent.click(screen.getByText("Second work"), { ctrlKey: true });
  fireEvent.click((await screen.findAllByRole("button", { name: /^Edit Smith 2020/ }))[0]!);
  const page = await screen.findByRole("textbox", { name: "Page" });
  await waitFor(() => expect(document.activeElement).toBe(page));
  fireEvent.click(screen.getByRole("button", { name: /Remove from Citation/ }));
  await waitFor(() => expect(document.querySelector(".inspector")).toBeNull());
  expect(document.activeElement).toBe(screen.getByLabelText("Search references"));
});

test("an output with no forms keeps its one line while a page is typed, with no pop-up in between", async () => {
  const reason = "Reference entries have no name forms. Choose another style.";
  const replies: ((value: unknown) => void)[] = [];
  backend({ search: () => [result("a", "Invented work")], preview_file_citation: ({ items }) => receipt(`Entry ${items[0]?.page_input ?? ""}`),
    citation_details: () => new Promise((resolve) => replies.push(resolve)) });
  render(<App />);
  const page = await openCitationDetails();
  await waitFor(() => expect(replies).toHaveLength(1));
  const none = { current_form: null, forms: [{ form: "normal", example: null, reason }, { form: "narrative", example: null, reason }, { form: "year-only", example: null, reason }] };
  await act(async () => replies[0]!(none));
  expect(screen.getByText(reason)).toBeTruthy();
  fireEvent.change(page, { target: { value: "9" } });
  await waitFor(() => expect(replies).toHaveLength(2));
  expect(screen.getByText(reason)).toBeTruthy();
  expect(screen.queryByRole("button", { name: /^Show as / })).toBeNull();
});
