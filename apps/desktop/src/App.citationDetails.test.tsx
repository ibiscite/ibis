import { afterEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import { backend, prepared, receipt, resetApp, result, openCitationDetails, closeCitationDetails, type PreviewRequest } from "./testing/app";

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
  const openReferences = () => {
    fireEvent.click(screen.getByRole("button", { name: "Show all 3 selected references" }));
    return screen.getByRole("dialog", { name: "Selected references" });
  };
  let list = openReferences();
  const names = () => within(list).getAllByRole("button", { name: /^Edit / }).map((button) => button.getAttribute("aria-label"));
  const order = names();
  fireEvent.click(within(list).getByRole("button", { name: "Edit Smith 2020 · b" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Page" }), { target: { value: "iv–vi" } });
  fireEvent.click(screen.getByRole("button", { name: "Remove from citation" }));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.getByRole("button", { name: "Show all 2 selected references" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Undo" }));
  list = openReferences();
  expect(names()).toEqual(order);
  fireEvent.click(within(list).getByRole("button", { name: "Edit Smith 2020 · b" }));
  expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Page" }).value).toBe("iv–vi");
});

test("closing and reopening while Page has focus returns to Search and retains the edited reference", async () => {
  backend({ search: () => [result("a", "Invented work")] });
  render(<App />);
  const page = await openCitationDetails();
  fireEvent.change(page, { target: { value: "42" } });
  await act(async () => { await emit("picker:hidden"); await emit("picker:shown"); await new Promise(requestAnimationFrame); });
  expect(document.activeElement).toBe(screen.getByLabelText("Search references"));
  expect(screen.queryByRole("dialog")).toBeNull();
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
  const actions = screen.getAllByRole("button", { name: "Undo" });
  expect(actions).toHaveLength(2);
  const descriptions = actions.map((button) => document.getElementById(button.getAttribute("aria-describedby")!)?.textContent);
  expect(descriptions).toEqual(["Removed from citation.", "Window position reset."]);
  for (const button of actions) { act(() => button.focus()); expect(document.activeElement).toBe(button); }
  await act(async () => { fireEvent.click(actions[1]!); });
  expect(windowUndos).toEqual([{ revision: 17 }]);
  expect(screen.queryByText("Window position reset.")).toBeNull();
  expect(screen.getByText("Removed from citation.")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Undo" }));
  expect(screen.queryByRole("button", { name: "Undo" })).toBeNull();
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
  const details = await screen.findByRole("dialog", { name: "Citation warnings" });
  for (const warning of warnings) expect(within(details).getByText(warning)).toBeTruthy();
  fireEvent.click(within(details).getByRole("button", { name: "Close" }));
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
  fireEvent.click(screen.getByRole("radio", { name: "Name in the sentence" }));
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
  await within(screen.getByRole("dialog")).findByText("Original output");
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
    fireEvent.click(screen.getByRole("button", { name: "Show all 2 selected references" }));
    const edit = within(screen.getByRole("dialog", { name: "Selected references" })).getByRole("button", { name: `Edit Smith 2020 · ${key}` });
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
  fireEvent.click(screen.getByRole("radio", { name: "Name in the sentence" }));
  await screen.findByRole("alert");
  if (!hideBefore) hide();
  expect(screen.getByRole("alert").textContent).toBe(message);
  fireEvent.keyDown(search, { key: "Enter" });
  await waitFor(() => expect(screen.getAllByRole("alert").map((alert) => alert.textContent)).toEqual([message]));
  expect(calls).not.toContain("copy_file_citation");
  fireEvent.click(screen.getByRole("radio", { name: "Name in brackets" }));
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
  fireEvent.click(screen.getByRole("radio", { name: "Name in the sentence" }));
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
  fireEvent.click(screen.getByRole("radio", { name: "Name in the sentence" }));
  closeCitationDetails();
  fireEvent.click(screen.getByText("Second work"), { ctrlKey: true });
  const edit = (key: string) => {
    fireEvent.click(screen.getByRole("button", { name: "Show all 2 selected references" }));
    return within(screen.getByRole("dialog", { name: "Selected references" })).getByRole("button", { name: `Edit Smith 2020 · ${key}` });
  };
  fireEvent.click(edit("b"));
  expect((screen.getByRole("textbox", { name: "Page" }) as HTMLInputElement).value).toBe("");
  fireEvent.change(screen.getByRole("textbox", { name: "Page" }), { target: { value: "29" } });
  fireEvent.click(screen.getByRole("radio", { name: "Name in the sentence" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Page" }), { target: { value: "" } });
  await waitFor(() => expect(previews.at(-1)?.items).toMatchObject([
    { id: result("a", "").id, locator: null, page_input: "iv–vi", form: "narrative" },
    { id: result("b", "").id, locator: null, form: "narrative" },
  ]));
  closeCitationDetails();
  fireEvent.click(edit("a"));
  expect((screen.getByRole("textbox", { name: "Page" }) as HTMLInputElement).value).toBe("iv–vi");
  expect(await screen.findByRole("radio", { name: "Name in the sentence", checked: true })).toBeTruthy();
  fireEvent.click(screen.getByRole("radio", { name: "Name in brackets" }));
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
  await within(screen.getByRole("dialog")).findByText(warning);
  closeCitationDetails();
  const preview = screen.getByRole("region", { name: "Preview" });
  const warnings = await within(preview).findByRole("button", { name: "1 warning" });
  expect(warnings.textContent).toBe("1 warning");
  fireEvent.click(warnings);
  expect(within(screen.getByRole("dialog", { name: "Citation warnings" })).getByText(warning)).toBeTruthy();
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  await openCitationDetails();
  fireEvent.click(screen.getByRole("radio", { name: "Name in the sentence" }));
  expect((await screen.findByRole("alert")).textContent).toContain("Narrative cannot be expressed");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  expect(calls).not.toContain("copy_file_citation");
  fireEvent.click(screen.getByRole("radio", { name: "Name in brackets" }));
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
  fireEvent.click(screen.getByRole("radio", { name: "Name in the sentence" }));
  await waitFor(() => expect(pending).toHaveLength(2));
  await act(async () => { pending[1]!(receipt("New narrative")); });
  await within(screen.getByRole("dialog")).findByText("New narrative");
  await act(async () => { pending[0]!(receipt("Retired normal")); });
  expect(screen.queryByText("Retired normal")).toBeNull();
  expect(calls).not.toContain("copy_file_citation");
});

test("changing only the form does not offer to reapply an unchanged search page", async () => {
  backend({ search: () => ({ revision: "1", results: [result("a", "Synthetic work")],
    interpretation: { search: "Synthetic", locator: { label: "page", value: "12" }, language: "en-US" } }) });
  render(<App />);
  await openCitationDetails();
  fireEvent.click(screen.getByRole("radio", { name: "Name in the sentence" }));
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
  fireEvent.click(screen.getByRole("radio", { name: "Name in the sentence" }));
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
  expect(screen.queryByText("Retired original example")).toBeNull();
  expect(screen.getByText("Current page 9 example")).toBeTruthy();
  closeCitationDetails();
  await openCitationDetails();
  await waitFor(() => expect(replies).toHaveLength(3));
  await act(async () => replies[2]!(choices("Reopened page 9 example")));
  expect(screen.getByText("Reopened page 9 example")).toBeTruthy();
  expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Page" }).value).toBe("9");
});

test("true Year only is sent to the engine; an engine refusal can be corrected from its owned menu", async () => {
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
  const year = await screen.findByRole("radio", { name: "Year only" });
  await waitFor(() => expect(year.matches(":disabled")).toBe(false));
  fireEvent.click(year);
  expect((await screen.findByRole("alert")).textContent).toContain("Year only cannot show");
  expect(previews.at(-1)?.items[0]?.form).toBe("year-only");
  const normal = screen.getByRole("radio", { name: "Name in brackets" });
  await waitFor(() => expect(normal.matches(":disabled")).toBe(false));
  fireEvent.click(normal);
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  expect(previews.at(-1)?.items[0]?.form).toBe("normal");
  expect(await screen.findByText("(Smith, 2020)", { selector: ".citation-form-example" })).toBeTruthy();
});
