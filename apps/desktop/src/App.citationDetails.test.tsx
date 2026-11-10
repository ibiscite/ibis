import { afterEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import { backend, chooseForm, chooseFromMore, prepared, receipt, resetApp, result, openCitationDetails, textField, closeCitationDetails, type PreviewRequest, inCitation, findInCitation, citationDetails, citationKeys, lineReceipt, removeFromCitation } from "./testing/app";

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

test("removing a middle reference and Undo restore its position and fields", async () => {
  backend({ search: () => [result("a", "First work"), result("b", "Second work"), result("c", "Third work")] });
  render(<App />);
  fireEvent.click(await screen.findByText("First work"), { ctrlKey: true });
  fireEvent.click(screen.getByText("Second work"), { ctrlKey: true });
  fireEvent.click(screen.getByText("Third work"), { ctrlKey: true });
  await waitFor(() => expect(citationKeys()).toEqual(["a", "b", "c"]));
  fireEvent.click(inCitation("b")!);
  fireEvent.change(await screen.findByRole("textbox", { name: "Page" }), { target: { value: "iv–vi" } });
  fireEvent.change(textField("Text before"), { target: { value: "see" } });
  fireEvent.change(textField("Text after"), { target: { value: "for details" } });
  removeFromCitation(inCitation("b"));
  expect(citationDetails()).toBeNull();
  await waitFor(() => expect(citationKeys()).toEqual(["a", "c"]));
  fireEvent.click(screen.getByRole("button", { name: /^Undo / }));
  await waitFor(() => expect(citationKeys()).toEqual(["a", "b", "c"]));
  fireEvent.click(inCitation("b")!);
  expect((await screen.findByRole<HTMLInputElement>("textbox", { name: "Page" })).value).toBe("iv–vi");
  expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Text before" }).value).toBe("see");
  expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Text after" }).value).toBe("for details");
});

test.each(["Text before", "Text after"] as const)("a late %s preview cannot replace the newest text or the receipt Enter copies", async (field) => {
  const previews = new Map<string, (preview: ReturnType<typeof receipt>) => void>();
  const copied: string[] = [];
  backend({
    search: () => [result("a", "Invented work")],
    preview_file_citation: ({ items }) => {
      const text = field === "Text before" ? items[0]?.prefix : items[0]?.suffix;
      if (!text) return receipt("Original citation");
      return new Promise((resolve) => { previews.set(text, resolve); });
    },
    copy_file_citation: (args) => {
      const { preview } = prepared(args);
      copied.push(preview.text);
      return { text: preview.text, warnings: [] };
    },
  });
  render(<App />);
  await openCitationDetails();
  const input = textField(field);
  fireEvent.change(input, { target: { value: "earlier" } });
  await waitFor(() => expect(previews.has("earlier")).toBe(true));
  fireEvent.change(input, { target: { value: "later" } });
  await waitFor(() => expect(previews.has("later")).toBe(true));
  expect(document.querySelector(".shelf-leading")?.getAttribute("draggable")).toBeNull();
  fireEvent.keyDown(input, { key: "Enter" });
  expect(copied).toEqual([]);
  await act(async () => { previews.get("later")!(receipt("New citation, later")); });
  await waitFor(() => expect(copied).toEqual(["New citation, later"]));
  await act(async () => { previews.get("earlier")!(receipt("Old citation, earlier")); });
  expect(copied).toEqual(["New citation, later"]);
  expect(screen.queryByText("Old citation, earlier")).toBeNull();
});

test("Keep My Order changes the engine preview and survives a short reopen", async () => {
  const copied: string[] = [];
  backend({
    picker_preferences: () => ({ output: "apa", density: "comfortable", larger_text: false, paste: true, menu_bar: true }),
    search: () => [result("z", "Zulu work"), result("a", "Alpha work")],
    preview_file_citation: ({ items, intent }) => {
      const keys = items.map(({ id }) => id.key);
      const order = intent.keep_order ? keys : [...keys].sort();
      return receipt(order.join("; "));
    },
    copy_file_citation: (args) => {
      const { preview } = prepared(args);
      copied.push(preview.text);
      return { text: preview.text, warnings: [] };
    },
  });
  render(<App />);
  fireEvent.click(await screen.findByText("Zulu work"), { ctrlKey: true });
  fireEvent.click(screen.getByText("Alpha work"), { ctrlKey: true });
  await waitFor(() => expect(screen.getByRole("region", { name: "Preview" }).textContent).toContain("a; z"));
  await chooseFromMore("Keep My Order", "menuitemcheckbox");
  expect(screen.getByRole("menuitemcheckbox", { name: "Keep My Order" }).getAttribute("aria-checked")).toBe("true");
  fireEvent.keyDown(screen.getByRole("menuitemcheckbox", { name: "Keep My Order" }), { key: "Escape" });
  await act(async () => { await emit("picker:hidden"); await emit("picker:shown", 2); });
  await waitFor(() => expect(screen.getByRole("region", { name: "Preview" }).textContent).toContain("z; a"));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await waitFor(() => expect(copied).toEqual(["z; a"]));
});

test("closing and reopening while Page has focus returns to Search and retains the edited reference", async () => {
  backend({ search: () => [result("a", "Invented work")] });
  render(<App />);
  const page = await openCitationDetails();
  fireEvent.change(page, { target: { value: "42" } });
  await act(async () => { await emit("picker:hidden"); await emit("picker:shown"); await new Promise(requestAnimationFrame); });
  expect(document.activeElement).toBe(screen.getByLabelText("Search references"));
  expect(screen.queryByRole("dialog", { name: /^Citation details for/ })).toBeNull();
  expect((await openCitationDetails()).value).toBe("42");
});

test("reference and window Undo take the slot in turn and cannot consume each other", async () => {
  const windowUndos: unknown[] = [];
  backend({ search: () => [result("a", "Invented work")], undo_window_position: (args) => { windowUndos.push(args); return true; } });
  render(<App />);
  const search = screen.getByLabelText("Search references");
  await screen.findByText("Invented work");
  fireEvent.keyDown(search, { key: "Enter", shiftKey: true });
  const segment = screen.getByRole("button", { name: "Edit Smith 2020" });
  act(() => segment.focus()); fireEvent.keyDown(segment, { key: "Backspace" });
  await act(async () => { await emit("picker:window-reset", { revision: 17, undo: 17 }); });
  // One slot: the newest offer shows, and the other waits with its action intact.
  const newest = screen.getByRole("button", { name: /^Undo / });
  expect(document.getElementById(newest.getAttribute("aria-describedby")!)?.textContent).toBe("Window position reset.");
  await act(async () => { fireEvent.click(newest); });
  expect(windowUndos).toEqual([{ revision: 17 }]);
  expect(screen.queryByText("Window position reset.")).toBeNull();
  expect(screen.getByText("Removed Smith 2020")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: /^Undo / }));
  expect(screen.queryByRole("button", { name: /^Undo / })).toBeNull();
  expect(inCitation("a")).toBeTruthy();
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
  await chooseForm("Smith (2020)");
  await waitFor(() => expect(previews.at(-1)?.items[0]).toEqual({
    id: result("a", "").id, cite: result("a", "").cite, locator, form: "narrative",
  }));
  expect(page.value).toBe(value);
});

test("a label-only Page refuses copy until corrected", async () => {
  const message = "Reference 1: Page needs a page or range. Enter one after the label, or clear Page.";
  const copied: string[] = [];
  const calls = backend({ search: () => [result("a", "Synthetic work")],
    preview_file_citation: ({ items }) => {
      if (items[0]?.locator_input?.text === "pp.") throw { code: "failed", message };
      return receipt(items[0]?.locator_input?.text ? "Synthetic normalized output" : "Original output");
    },
    copy_file_citation: (args) => { const saved = prepared(args); copied.push(saved.preview.text); return { text: saved.preview.text, warnings: [] }; },
  });
  render(<App />);
  const page = await openCitationDetails();
  await within(screen.getByRole("region", { name: "Preview" })).findByText("Original output");
  const search = screen.getByLabelText("Search references");
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

test.each(["pointer", " ", "ArrowDown"])("a token activated with %s opens its details with Page focused and edits that reference", async (activation) => {
  const previews: PreviewRequest[] = [];
  const calls = backend({ search: () => [result("a", "First work"), result("b", "Second work")],
    preview_file_citation: (request) => { previews.push(request); return lineReceipt(request.items.map(({ id }) => `(${id.key})`)); } });
  render(<App />);
  fireEvent.click(await screen.findByText("First work"), { ctrlKey: true });
  fireEvent.click(screen.getByText("Second work"), { ctrlKey: true });
  for (const [key, title, value] of [["b", "Second work", "29"], ["a", "First work", "12–15"], ["a", "First work", "16"]]) {
    const edit = await findInCitation(key!);
    if (activation === "pointer") fireEvent.click(edit);
    else {
      const code = activation === " " ? "Space" : activation;
      act(() => edit.focus());
      fireEvent.keyDown(edit, { key: activation, code });
      if (activation === " ") fireEvent.keyUp(edit, { key: activation, code });
    }
    const page = await screen.findByRole("textbox", { name: "Page" });
    expect(document.activeElement).toBe(page);
    expect(screen.getByRole("dialog", { name: `Citation details for ${title}` }).contains(page)).toBe(true);
    fireEvent.change(page, { target: { value } });
    await waitFor(() => expect(previews.at(-1)?.items.find((item) => item.id.key === key)?.locator_input?.text).toBe(value));
    closeCitationDetails();
  }
  expect(previews.at(-1)?.items).toMatchObject([
    { id: result("a", "").id, locator: null, locator_input: { label: "page", text: "16" } },
    { id: result("b", "").id, locator: null, locator_input: { label: "page", text: "29" } },
  ]);
  expect(calls).not.toContain("copy_file_citation");
});

test("actionable refusals remain visible until corrected", async () => {
  const message = "Reference 1: Narrative cannot be expressed in this output. Choose Normal or another output.";
  const calls = backend({ search: () => [result("a", "Synthetic work")],
    preview_file_citation: ({ items }) => {
      if (items[0]?.form === "narrative") throw { code: "failed", message };
      return receipt("Synthetic output");
    } });
  render(<App />);
  await openCitationDetails();
  const search = screen.getByLabelText("Search references");
  await chooseForm("Smith (2020)");
  expect((await screen.findByRole("alert")).textContent).toBe(message);
  fireEvent.keyDown(search, { key: "Enter" });
  await waitFor(() => expect(screen.getAllByRole("alert").map((alert) => alert.textContent)).toEqual([message]));
  expect(calls).not.toContain("copy_file_citation");
  await chooseForm("(Smith, 2020)");
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
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
  await chooseForm("Smith (2020)");
  await waitFor(() => expect(previews.at(-1)?.items).toMatchObject([
    { id: result("a", "").id, locator: null, locator_input: { label: "page", text: "12–15" }, form: "narrative" },
  ]));
  const search = screen.getByLabelText("Search references");
  fireEvent.change(search, { target: { value: "second" } });
  await screen.findByText("Second work");
  expect((page as HTMLInputElement).value).toBe("12–15");
  fireEvent.keyDown(search, { key: "Enter" });
  await waitFor(() => expect(copies).toHaveLength(1));
  expect(copies[0]?.items).toMatchObject([{ id: result("a", "").id, locator: null, locator_input: { label: "page", text: "12–15" }, form: "narrative" }]);
});

test("each shelf reference keeps independent details and an empty Page removes only that page", async () => {
  const previews: PreviewRequest[] = [];
  backend({ search: () => [result("a", "First work"), result("b", "Second work")],
    preview_file_citation: (request) => { previews.push(request); return lineReceipt(request.items.map(({ id }) => `(${id.key})`)); } });
  render(<App />);
  fireEvent.click(await screen.findByText("First work"), { ctrlKey: true });
  await openCitationDetails();
  fireEvent.change(screen.getByRole("textbox", { name: "Page" }), { target: { value: "iv–vi" } });
  await chooseForm("Smith (2020)");
  closeCitationDetails();
  fireEvent.click(screen.getByText("Second work"), { ctrlKey: true });
  const edit = findInCitation;
  fireEvent.click(await edit("b"));
  expect((await screen.findByRole<HTMLInputElement>("textbox", { name: "Page" })).value).toBe("");
  fireEvent.change(screen.getByRole("textbox", { name: "Page" }), { target: { value: "29" } });
  await chooseForm("Smith (2020)");
  fireEvent.change(screen.getByRole("textbox", { name: "Page" }), { target: { value: "" } });
  await waitFor(() => expect(previews.at(-1)?.items).toMatchObject([
    { id: result("a", "").id, locator: null, locator_input: { label: "page", text: "iv–vi" }, form: "narrative" },
    { id: result("b", "").id, locator: null, form: "narrative" },
  ]));
  closeCitationDetails();
  fireEvent.click(await edit("a"));
  expect((await screen.findByRole<HTMLInputElement>("textbox", { name: "Page" })).value).toBe("iv–vi");
  expect(isChosen("Smith (2020)")).toBe(true);
  await chooseForm("(Smith, 2020)");
  await waitFor(() => expect(previews.at(-1)?.items[0]?.form).toBe("normal"));
});

test("losses keep a visible count and an unsupported form blocks copying until corrected", async () => {
  const warning = "Reference 1: Page is not included in this output.";
  const calls = backend({ search: () => [result("a", "Synthetic work")],
    preview_file_citation: ({ items }) => {
      if (items[0]?.form === "narrative") throw { code: "failed", message: "Reference 1: Narrative cannot be expressed in this output." };
      return receipt("Synthetic output", null, items[0]?.locator_input?.text ? [warning] : []);
    } });
  render(<App />);
  const page = await openCitationDetails();
  fireEvent.change(page, { target: { value: "12" } });
  await within(screen.getByRole("dialog", { name: /^Citation details for/ })).findByText(warning);
  closeCitationDetails();
  const preview = screen.getByRole("region", { name: "Preview" });
  const warnings = await within(preview).findByRole("button", { name: "1 warning" });
  expect(warnings.textContent).toBe("1 warning");
  fireEvent.click(warnings);
  const notes = screen.getByRole("region", { name: "Citation warnings" });
  expect(within(notes).getByText(warning)).toBeTruthy();
  fireEvent.keyDown(notes, { key: "Escape" });
  await openCitationDetails();
  await chooseForm("Smith (2020)");
  expect((await screen.findByRole("alert")).textContent).toContain("Narrative cannot be expressed");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  expect(calls).not.toContain("copy_file_citation");
  await chooseForm("(Smith, 2020)");
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
  await within(screen.getByRole("region", { name: "Preview" })).findByText("New narrative");
  await act(async () => { pending[0]!(receipt("Retired normal")); });
  expect(screen.queryByText("Retired normal")).toBeNull();
  expect(calls).not.toContain("copy_file_citation");
});

test("changing only the form does not offer to reapply an unchanged search page", async () => {
  backend({ search: () => ({ revision: "1", results: [result("a", "Synthetic work")],
    interpretation: { search: "Synthetic", locator: { label: "page", value: "12" }, language: "en-US" } }) });
  render(<App />);
  await openCitationDetails();
  await chooseForm("Smith (2020)");
  closeCitationDetails();
  const reopenedPage = await openCitationDetails();
  expect(reopenedPage.value).toBe("12");
  closeCitationDetails();
  expect(screen.queryByRole("button", { name: "Apply locator to a" })).toBeNull();
});

test("page and form survive changing output and remain editable", async () => {
  const previews: PreviewRequest[] = [];
  backend({ search: () => [result("a", "Synthetic work")],
    preview_file_citation: (request) => { previews.push(request); return receipt(`preview ${previews.length}`); } });
  render(<App />);
  let page = await openCitationDetails();
  fireEvent.change(page, { target: { value: "42" } });
  await chooseForm("Smith (2020)");
  const search = screen.getByLabelText("Search references");
  closeCitationDetails();
  fireEvent.click(within(search.closest("header")!).getByRole("button", { name: "Copy as Pandoc" }));
  fireEvent.click(screen.getByRole("menuitemradio", { name: "Typst" }));
  await waitFor(() => expect(previews.at(-1)?.output).toBe("typst"));
  expect(previews.at(-1)?.items[0]).toMatchObject({ locator: null, locator_input: { label: "page", text: "42" }, form: "narrative" });
  page = await openCitationDetails();
  fireEvent.change(page, { target: { value: "42–45" } });
  await waitFor(() => expect(previews.at(-1)?.items[0]?.locator_input?.text).toBe("42–45"));
  expect(previews.at(-1)?.items[0]?.form).toBe("narrative");
});

test("late form metadata cannot overwrite the examples for a newer Page edit or reopened details", async () => {
  const replies: ((value: unknown) => void)[] = [];
  const requests: { ticket: string; item: number }[] = [];
  backend({ search: () => [result("a", "Synthetic metadata work")],
    preview_file_citation: ({ items }) => receipt(`Page ${items[0]?.locator_input?.text ?? "original"}`),
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
  expect(screen.getByRole("radio", { name: "Current page 9 example" })).toBeTruthy();
  expect(screen.queryByText("Retired original example")).toBeNull();
  closeCitationDetails();
  await openCitationDetails();
  await waitFor(() => expect(replies).toHaveLength(3));
  await act(async () => replies[2]!(choices("Reopened page 9 example")));
  expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Page" }).value).toBe("9");
  expect(screen.getByRole("radio", { name: "Reopened page 9 example" })).toBeTruthy();
});

test("true Year only is sent to the engine; an engine refusal can be corrected from the details", async () => {
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
  await chooseForm("(2020)");
  expect((await screen.findByRole("alert")).textContent).toContain("Year only cannot show");
  expect(previews.at(-1)?.items[0]?.form).toBe("year-only");
  await chooseForm("(Smith, 2020)");
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  expect(previews.at(-1)?.items[0]?.form).toBe("normal");
  expect(isChosen("(Smith, 2020)")).toBe(true);
});

test("the details open beside a live list; Esc steps back one layer and returns to the token", async () => {
  backend({ search: () => ({ revision: "1", results: [result("a", "Invented work")] }), hide_picker: () => true });
  render(<App />);
  const search = screen.getByLabelText("Search references");
  await screen.findByText("Invented work");
  fireEvent.keyDown(search, { key: "Enter", shiftKey: true });
  const segment = await screen.findByRole("button", { name: "Edit Smith 2020" });
  act(() => segment.focus());
  fireEvent.keyDown(segment, { key: "ArrowDown" });
  const page = await screen.findByRole("textbox", { name: "Page" });
  expect(document.activeElement).toBe(page);
  expect(screen.getByRole("option")).toBeTruthy();
  expect(segment.getAttribute("aria-expanded")).toBe("true");
  expect(document.getElementById(segment.getAttribute("aria-controls")!)?.contains(page)).toBe(true);
  expect(screen.getByRole("dialog", { name: "Citation details for Invented work" }).contains(page)).toBe(true);
  fireEvent.keyDown(page, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("textbox", { name: "Page" })).toBeNull());
  expect(document.activeElement).toBe(segment);
  // React Aria saw the focus, so the keyboard ring draws.
  expect(segment.hasAttribute("data-focus-visible")).toBe(true);
  expect(segment.getAttribute("aria-expanded")).toBe("false");
});

test("Search and the list stay live beside the details, and Esc from Search closes them first", async () => {
  const calls = backend({ search: () => [result("a", "First work"), result("b", "Second work")], hide_picker: () => true });
  render(<App />);
  fireEvent.click(await screen.findByText("First work"), { ctrlKey: true });
  await openCitationDetails();
  const search = screen.getByLabelText("Search references");
  act(() => search.focus());
  expect(screen.getByRole("textbox", { name: "Page" })).toBeTruthy();
  fireEvent.keyDown(search, { key: "ArrowDown" });
  await waitFor(() => expect(screen.getByRole("option", { name: /Second work/ }).getAttribute("aria-selected")).toBe("true"));
  expect(screen.getByRole("dialog", { name: "Citation details for First work" })).toBeTruthy();
  fireEvent.keyDown(search, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("textbox", { name: "Page" })).toBeNull());
  expect(document.activeElement).toBe(search);
  expect(calls).not.toContain("hide_picker");
});

test("a citation refusal shows once, in the details beside its fields, and returns to the list when they close", async () => {
  const message = "Reference 1: Page needs a page or range.";
  backend({ search: () => [result("a", "Synthetic work")], preview_file_citation: () => { throw { code: "failed", message }; } });
  render(<App />);
  const inList = await screen.findByRole("alert");
  expect(inList.closest(".notices")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Details" }));
  const alerts = await screen.findAllByRole("alert");
  expect(alerts.map((alert) => alert.textContent)).toEqual([message]);
  expect(citationDetails()?.contains(alerts[0]!)).toBe(true);
  closeCitationDetails();
  await waitFor(() => expect(citationDetails()).toBeNull());
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

test("a form pick keeps focus in the details, so the next key stays there", async () => {
  backend({ search: () => [result("a", "Invented work")] });
  render(<App />);
  await openCitationDetails();
  const form = await waitFor(() => screen.getByRole("radio", { name: "(2020)" }));
  act(() => form.focus());
  fireEvent.click(form);
  await waitFor(() => expect(isChosen("(2020)")).toBe(true));
  expect(document.activeElement).toBe(form);
  fireEvent.keyDown(form, { key: "a" });
  expect(screen.getByRole("textbox", { name: "Page" })).toBeTruthy();
});

test("Enter in Page does what the footer says, once, and a held key does not repeat it", async () => {
  const copied: string[] = [];
  backend({ search: () => [result("a", "Invented work")], preview_file_citation: ({ items }) => receipt(`Citation ${items[0]?.locator_input?.text ?? ""}`.trim()),
    copy_file_citation: (args) => { const saved = prepared(args); copied.push(saved.preview.text); return { text: saved.preview.text, warnings: [] }; } });
  render(<App />);
  const page = await openCitationDetails();
  fireEvent.change(page, { target: { value: "42" } });
  await within(screen.getByRole("region", { name: "Preview" })).findByText("Citation 42");
  fireEvent.keyDown(page, { key: "Enter", repeat: true });
  fireEvent.keyDown(page, { key: "Enter", isComposing: true });
  expect(copied).toEqual([]);
  fireEvent.keyDown(page, { key: "Enter" });
  await waitFor(() => expect(copied).toEqual(["Citation 42"]));
});

test("a refused page stays visible in place, and Enter in Page copies nothing", async () => {
  const message = "Reference 1: Page needs a page or range.";
  const calls = backend({ search: () => [result("a", "Synthetic work")], preview_file_citation: ({ items }) => {
    if (items[0]?.locator_input?.text === "pp.") throw { code: "failed", message };
    return receipt("Original output");
  } });
  render(<App />);
  const page = await openCitationDetails();
  fireEvent.change(page, { target: { value: "pp." } });
  const refusal = await within(screen.getByRole("dialog", { name: /^Citation details for/ })).findByRole("alert");
  expect(refusal.textContent).toBe(message);
  expect(document.activeElement).toBe(page);
  fireEvent.keyDown(page, { key: "Enter" });
  await act(async () => { await Promise.resolve(); });
  expect(calls).not.toContain("copy_file_citation");
  expect(document.activeElement).toBe(page);
  expect(within(screen.getByRole("dialog", { name: /^Citation details for/ })).getByRole("alert").textContent).toBe(message);
});

test("failed examples leave every form usable and say so once, quietly", async () => {
  const previews: PreviewRequest[] = [];
  backend({ search: () => [result("a", "Invented work")], citation_details: () => { throw { code: "failed", message: "Invented failure" }; },
    preview_file_citation: (request) => { previews.push(request); return receipt("Output"); } });
  render(<App />);
  await openCitationDetails();
  expect(await screen.findByText("Examples are unavailable")).toBeTruthy();
  fireEvent.click(screen.getByRole("radio", { name: "Year only" }));
  await waitFor(() => expect(previews.at(-1)?.items[0]?.form).toBe("year-only"));
  expect(screen.queryByRole("alert")).toBeNull();
});

test("cancelled examples say nothing: a newer preview owns the answer", async () => {
  const previews: PreviewRequest[] = [];
  backend({ search: () => [result("a", "Invented work")], citation_details: () => { throw { code: "cancelled" }; },
    preview_file_citation: (request) => { previews.push(request); return receipt("Output"); } });
  render(<App />);
  await openCitationDetails();
  await act(async () => { await Promise.resolve(); });
  expect(screen.queryByText("Examples are unavailable")).toBeNull();
  fireEvent.click(screen.getByRole("radio", { name: "Year only" }));
  await waitFor(() => expect(previews.at(-1)?.items[0]?.form).toBe("year-only"));
  expect(screen.queryByRole("alert")).toBeNull();
});

test("examples from the last answer stay while a page is typed, so the choices never empty", async () => {
  const replies: ((value: unknown) => void)[] = [];
  backend({ search: () => [result("a", "Invented work")], preview_file_citation: ({ items }) => receipt(`Page ${items[0]?.locator_input?.text ?? ""}`),
    citation_details: () => new Promise((resolve) => replies.push(resolve)) });
  render(<App />);
  const page = await openCitationDetails();
  await waitFor(() => expect(replies).toHaveLength(1));
  await act(async () => replies[0]!({ current_form: "normal", forms: [{ form: "normal", example: "First example", reason: null }, { form: "narrative", example: null, reason: null }, { form: "year-only", example: null, reason: null }] }));
  fireEvent.change(page, { target: { value: "9" } });
  await waitFor(() => expect(replies).toHaveLength(2));
  expect(screen.getByRole("radio", { name: "First example" })).toBeTruthy();
});

test("pressing another token moves the details to it and retires the first reference's examples", async () => {
  const replies: { item: number; resolve: (value: unknown) => void }[] = [];
  backend({ search: () => [result("a", "First work"), result("b", "Second work")],
    preview_file_citation: (request) => ({ ...receipt("(First; Second)"), spans: [{ item: 0, text: "First" }, { item: null, text: "; " }, { item: 1, text: "Second" }].slice(0, request.items.length * 2 - 1) }),
    citation_details: (args) => new Promise((resolve) => replies.push({ item: Number(args.item), resolve })) });
  render(<App />);
  fireEvent.click(await screen.findByText("First work"), { ctrlKey: true });
  fireEvent.click(screen.getByText("Second work"), { ctrlKey: true });
  fireEvent.click(await screen.findByRole("button", { name: "Edit Smith 2020 · a" }));
  await screen.findByRole("dialog", { name: "Citation details for First work" });
  await waitFor(() => expect(replies.at(-1)?.item).toBe(0));
  const first = replies.at(-1)!;
  fireEvent.click(screen.getByRole("button", { name: "Edit Smith 2020 · b" }));
  await screen.findByRole("dialog", { name: "Citation details for Second work" });
  await waitFor(() => expect(replies.at(-1)?.item).toBe(1));
  const choices = (example: string) => ({ current_form: "normal", forms: [{ form: "normal", example, reason: null }, { form: "narrative", example: null, reason: null }, { form: "year-only", example: null, reason: null }] });
  await act(async () => replies.at(-1)!.resolve(choices("Second example")));
  await act(async () => first.resolve(choices("Retired first example")));
  expect(screen.getByRole("radio", { name: "Second example" })).toBeTruthy();
  expect(screen.queryByText("Retired first example")).toBeNull();
});

test("a form picked while no receipt exists shows the form asked for, and an old reason cannot block it", async () => {
  const previews: PreviewRequest[] = [];
  backend({ search: () => [result("a", "Synthetic work")],
    citation_details: () => ({ current_form: "normal", forms: [{ form: "normal", example: "(Smith, 2020)", reason: null }, { form: "narrative", example: null, reason: "Cite it alone." }, { form: "year-only", example: "(2020)", reason: null }] }),
    preview_file_citation: (request) => {
      previews.push(request);
      if (request.items[0]?.locator_input?.text === "pp.") throw { code: "failed", message: "Reference 1: Page needs a page or range." };
      return receipt("(Smith, 2020)");
    } });
  render(<App />);
  const page = await openCitationDetails();
  await act(async () => { await Promise.resolve(); });
  fireEvent.change(page, { target: { value: "pp." } });
  await screen.findByRole("alert");
  await chooseForm("Name in the sentence");
  await waitFor(() => expect(isChosen("Name in the sentence")).toBe(true));
  await waitFor(() => expect(previews.at(-1)?.items[0]?.form).toBe("narrative"));
});

test("a copy that fails says so in the details, once, and in the list when they close", async () => {
  backend({ search: () => [result("a", "Invented work")], preview_file_citation: () => receipt("Citation"), copy_file_citation: () => { throw new Error("SYNTHETIC_PRIVATE_PAYLOAD"); } });
  render(<App />);
  const page = await openCitationDetails();
  await within(screen.getByRole("region", { name: "Preview" })).findByText("Citation");
  fireEvent.keyDown(page, { key: "Enter" });
  const alert = await screen.findByRole("alert");
  expect(citationDetails()?.contains(alert)).toBe(true);
  expect(alert.textContent).not.toContain("SYNTHETIC_PRIVATE_PAYLOAD");
  expect(screen.getByRole("region", { name: "Preview" }).textContent).toBe("Citation");
  closeCitationDetails();
  await waitFor(() => expect(citationDetails()).toBeNull());
  expect((await screen.findByRole("alert")).closest(".notices")).toBeTruthy();
});

test.each([1, 2])("removing the edited reference closes its details and returns to Search (%s chosen)", async (count) => {
  backend({ search: () => [result("a", "First work"), result("b", "Second work")] });
  render(<App />);
  fireEvent.click(await screen.findByText("First work"), { ctrlKey: true });
  if (count === 2) fireEvent.click(screen.getByText("Second work"), { ctrlKey: true });
  fireEvent.click((await screen.findAllByRole("button", { name: /^Edit Smith 2020/ }))[0]!);
  const page = await screen.findByRole("textbox", { name: "Page" });
  await waitFor(() => expect(document.activeElement).toBe(page));
  removeFromCitation(inCitation("a"));
  await waitFor(() => expect(citationDetails()).toBeNull());
  expect(inCitation("a")).toBeNull();
  expect(document.activeElement).toBe(screen.getByLabelText("Search references"));
});

test("an output with no forms keeps its one line while a page is typed, with no pop-up in between", async () => {
  const reason = "Reference entries have no name forms. Choose another style.";
  const replies: ((value: unknown) => void)[] = [];
  backend({ search: () => [result("a", "Invented work")], preview_file_citation: ({ items }) => receipt(`Entry ${items[0]?.locator_input?.text ?? ""}`),
    citation_details: () => new Promise((resolve) => replies.push(resolve)) });
  render(<App />);
  const page = await openCitationDetails();
  await waitFor(() => expect(replies).toHaveLength(1));
  const none = { current_form: null, forms: (["normal", "narrative", "author-only", "year-only", "full"] as const).map((form) => ({ form, example: null, reason })) };
  await act(async () => replies[0]!(none));
  expect(screen.getByText(reason)).toBeTruthy();
  fireEvent.change(page, { target: { value: "9" } });
  await waitFor(() => expect(replies).toHaveLength(2));
  expect(screen.getByText(reason)).toBeTruthy();
  expect(screen.queryByRole("radiogroup", { name: "Show as" })).toBeNull();
});

/** A style that sorts its references by key, unless the person keeps their order.
 * With `held`, each preview waits under "kept:" or "sorted:" and its order until released. */
function sortingStyle(held?: Map<string, () => void>) {
  return ({ items, intent }: PreviewRequest) => {
    const order = items.map(({ id }, index) => ({ key: id.key, index }));
    if (!intent.keep_order) order.sort((x, y) => x.key.localeCompare(y.key));
    const spans = order.flatMap(({ key, index }, n) => [...n ? [{ item: null, text: "; " }] : [], { item: index, text: `(${key})` }]);
    const preview = { ...receipt(spans.map((span) => span.text).join("")), spans };
    if (!held) return preview;
    return new Promise<typeof preview>((resolve) => held.set(`${intent.keep_order ? "kept" : "sorted"}:${order.map(({ key }) => key).join(",")}`, () => resolve(preview)));
  };
}
const isChosen = (name: string) => screen.getByRole<HTMLInputElement>("radio", { name }).checked;
const line = () => screen.getByRole("region", { name: "Preview" }).querySelector(".shelf-citation")?.textContent;
const apa = () => ({ output: "apa", density: "comfortable", larger_text: false, paste: true, menu_bar: true });

test("⇧→ moves a token in a sorting style: the line shows the new order, focus stays on it, and Keep My Order off restores the style's", async () => {
  backend({ picker_preferences: apa, search: () => [result("c", "Third work"), result("a", "First work"), result("b", "Second work")], preview_file_citation: sortingStyle() });
  render(<App />);
  for (const title of ["Third work", "First work", "Second work"]) fireEvent.click(await screen.findByText(title), { ctrlKey: true });
  await waitFor(() => expect(line()).toBe("(a); (b); (c)"));
  const a = inCitation("a")!;
  act(() => a.focus());
  fireEvent.keyDown(a, { key: "ArrowRight", shiftKey: true });
  await waitFor(() => expect(line()).toBe("(b); (a); (c)"));
  expect(document.activeElement).toBe(inCitation("a"));
  await chooseFromMore("Keep My Order", "menuitemcheckbox");
  expect(screen.getByRole("menuitemcheckbox", { name: "Keep My Order" }).getAttribute("aria-checked")).toBe("false");
  fireEvent.keyDown(screen.getByRole("menuitemcheckbox", { name: "Keep My Order" }), { key: "Escape" });
  await waitFor(() => expect(line()).toBe("(a); (b); (c)"));
});

test("while a move keeps the person's order in a sorting style, the footer's action says so and the line does not", async () => {
  backend({ picker_preferences: apa, search: () => [result("z", "Zulu work"), result("a", "Alpha work")], preview_file_citation: sortingStyle() });
  render(<App />);
  fireEvent.click(await screen.findByText("Zulu work"), { ctrlKey: true });
  fireEvent.click(screen.getByText("Alpha work"), { ctrlKey: true });
  await waitFor(() => expect(line()).toBe("(a); (z)"));
  fireEvent.keyDown(inCitation("a")!, { key: "ArrowRight", shiftKey: true });
  await waitFor(() => expect(line()).toBe("(z); (a)"));
  expect(within(screen.getByRole("group", { name: "Keys" })).getByRole("button", { name: /^Copy 2 as APA · your order/ })).toBeTruthy();
  expect(screen.getByRole("region", { name: "Preview" }).textContent).not.toContain("your order");
});

test("⌘↓ in Search goes to the last token; ↓ opens its details at the locator, Esc returns to it, ↑ returns to Search", async () => {
  backend({ search: () => [result("a", "First work"), result("b", "Second work")] });
  render(<App />);
  fireEvent.click(await screen.findByText("First work"), { ctrlKey: true });
  fireEvent.click(screen.getByText("Second work"), { ctrlKey: true });
  const search = screen.getByLabelText("Search references");
  const last = await findInCitation("b");
  act(() => search.focus());
  fireEvent.keyDown(search, { key: "ArrowDown", ctrlKey: true });
  expect(document.activeElement).toBe(last);
  fireEvent.keyDown(last, { key: "ArrowDown" });
  const details = await screen.findByRole("dialog", { name: "Citation details for Second work" });
  await waitFor(() => expect(document.activeElement).toBe(within(details).getByRole("textbox", { name: "Page" })));
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  await waitFor(() => expect(citationDetails()).toBeNull());
  expect(document.activeElement).toBe(last);
  fireEvent.keyDown(last, { key: "ArrowUp" });
  expect(document.activeElement).toBe(search);
});

test("More's Move Left and Remove from Citation act on the last token while Search has focus", async () => {
  backend({ search: () => [result("a", "First work"), result("b", "Second work")] });
  render(<App />);
  fireEvent.click(await screen.findByText("First work"), { ctrlKey: true });
  fireEvent.click(screen.getByText("Second work"), { ctrlKey: true });
  await waitFor(() => expect(citationKeys()).toEqual(["a", "b"]));
  act(() => screen.getByLabelText("Search references").focus());
  await chooseFromMore("Move Left");
  await waitFor(() => expect(citationKeys()).toEqual(["b", "a"]));
  act(() => screen.getByLabelText("Search references").focus());
  await chooseFromMore("Remove from Citation");
  await waitFor(() => expect(citationKeys()).toEqual(["b"]));
});

test("a late preview from before Keep My Order went off cannot replace the newest line or the receipt Enter copies", async () => {
  const held = new Map<string, () => void>();
  const copied: string[] = [];
  backend({
    picker_preferences: apa,
    search: () => [result("z", "Zulu work"), result("a", "Alpha work")],
    preview_file_citation: sortingStyle(held),
    copy_file_citation: (args) => { const { preview } = prepared(args); copied.push(preview.text); return { text: preview.text, warnings: [] }; },
  });
  render(<App />);
  fireEvent.click(await screen.findByText("Zulu work"), { ctrlKey: true });
  fireEvent.click(screen.getByText("Alpha work"), { ctrlKey: true });
  await waitFor(() => expect(held.has("sorted:a,z")).toBe(true));
  await act(async () => { held.get("sorted:a,z")!(); });
  held.clear();
  await waitFor(() => expect(line()).toBe("(a); (z)"));
  fireEvent.keyDown(inCitation("a")!, { key: "ArrowRight", shiftKey: true });
  await waitFor(() => expect(held.has("kept:z,a")).toBe(true));
  await chooseFromMore("Keep My Order", "menuitemcheckbox");
  fireEvent.keyDown(screen.getByRole("menuitemcheckbox", { name: "Keep My Order" }), { key: "Escape" });
  await waitFor(() => expect(held.has("sorted:a,z")).toBe(true));
  const search = screen.getByLabelText("Search references");
  act(() => search.focus());
  fireEvent.keyDown(search, { key: "Enter" });
  expect(copied).toEqual([]);
  await act(async () => { held.get("sorted:a,z")!(); });
  await waitFor(() => expect(copied).toEqual(["(a); (z)"]));
  await act(async () => { held.get("kept:z,a")!(); });
  expect(copied).toEqual(["(a); (z)"]);
  expect(screen.queryByText("(z); (a)")).toBeNull();
});
