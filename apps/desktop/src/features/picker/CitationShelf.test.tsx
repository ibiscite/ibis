import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createRef, useState } from "react";
import type { SearchResult } from "../../api";
import type { CitationReference } from "./choice";
import { referenceLabels, CitationShelf } from "./CitationShelf";

afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

function record(key: string, first_author = "García", year: number | null = 2021): SearchResult {
  return { revision: "1", id: { source: "invented.bib", native_key: key }, key, title: `Invented title ${key}`, authors: first_author,
    year, matched: ["title"], partial: false, first_author, author_count: 1, container: null, item_type: "book" };
}
const chosen = (records: CitationReference[]) => new Map(records.map((r) => [r.key, r]));
const output = (text = "(García, 2021)") => ({ markup: [], text, html: "", warnings: [] });
const shelf = (props: Partial<Parameters<typeof CitationShelf>[0]>) => <CitationShelf announce={false} selected={new Map()}
  removeLabel={(r) => `Remove ${r.key}`} onRemove={() => undefined} onChangeReference={() => undefined}
  style="apa" keys={false} shown={undefined} focusable={false} outputWarnings={[]} {...props} />;
const press = (node: HTMLElement, key: string) => {
  act(() => node.focus());
  fireEvent.keyDown(node, { key, code: key === " " ? "Space" : key });
  fireEvent.keyUp(node, { key, code: key === " " ? "Space" : key });
};

test("engine-owned segments retain punctuation, per-reference removal and arrow selection", () => {
  const a = record("a", "Alpha"), b = record("b", "Beta"), remove = vi.fn();
  const formatted = { ...output("(Beta, 2021; Alpha, 2021)"), spans: [
    { item: null, text: "(" }, { item: 1, text: "Beta, 2021" }, { item: null, text: "; " },
    { item: 0, text: "Alpha, 2021" }, { item: null, text: ")" },
  ] };
  render(shelf({ selected: chosen([a, b]), shown: formatted, onRemove: remove }));
  const beta = screen.getByRole("button", { name: "Edit Beta 2021" });
  const alpha = screen.getByRole("button", { name: "Edit Alpha 2021" });
  expect(document.querySelector(".shelf-citation")?.textContent).toBe(formatted.text);
  act(() => beta.focus());
  fireEvent.keyDown(beta, { key: "ArrowRight" });
  expect(document.activeElement).toBe(alpha);
  fireEvent.keyDown(alpha, { key: "Delete" });
  expect(remove).toHaveBeenCalledExactlyOnceWith(a);
  expect(within(beta.closest(".shelf-segment")!).getByRole("button", { name: "Remove b" })).toBeTruthy();
});

test("formatted segments preserve engine emphasis and keep source strings inert", () => {
  const a = record("a", "Alpha"), b = record("b", "Beta");
  const text = "Alpha species; <script>literal</script>";
  render(shelf({ selected: chosen([a, b]), shown: { ...output(text), spans: [
    { item: 0, text: "Alpha species", runs: [{ text: "Alpha species", italic: true, bold: false, script: "normal" }] },
    { item: null, text: "; " },
    { item: 1, text: "<script>literal</script>", runs: [{ text: "<script>literal</script>", italic: false, bold: true, script: "normal" }] },
  ] } }));
  expect(document.querySelector(".shelf-citation")?.textContent).toBe(text);
  expect(document.querySelector(".shelf-citation i")?.textContent).toBe("Alpha species");
  expect(document.querySelector(".shelf-citation b")?.textContent).toBe("<script>literal</script>");
  expect(document.querySelector(".shelf script")).toBeNull();
});

test("short names distinguish identical author/year labels without using the citation key as identity", () => {
  expect(referenceLabels([record("a"), record("b", "Okafor", 2019), record("c", "", null)])).toEqual(["García 2021", "Okafor 2019", "c"]);
  expect(referenceLabels([record("a"), record("b")])).toEqual(["García 2021 · a", "García 2021 · b"]);
});

test("formatted segments keep passive small caps and underline from the engine", () => {
  render(shelf({ selected: chosen([record("a"), record("b")]), shown: { ...output("Caps; Under"), spans: [
    { item: 0, text: "Caps", runs: [{ text: "Caps", small_caps: true }] },
    { item: null, text: "; " }, { item: 1, text: "Under", runs: [{ text: "Under", underline: true }] },
  ] } }));
  expect(document.querySelector<HTMLElement>('.shelf [style*="small-caps"]')?.textContent).toBe("Caps");
  expect(document.querySelector<HTMLElement>('.shelf [style*="underline"]')?.textContent).toBe("Under");
});

test("colliding keys from different libraries have distinct visible and screen-reader names", () => {
  const first = { ...record("shared"), id: { source: "invented-one", key: "shared" }, source: "invented-one", title: "First invented work" };
  const second = { ...first, id: { source: "invented-two", key: "shared" }, source: "invented-two", title: "Second invented work" };
  const change = vi.fn();
  render(shelf({ selected: new Map([["first", first], ["second", second]]), onChangeReference: change,
    referenceLibrary: (r) => r.source === "invented-one" ? "First library" : "Second library", shown: output() }));
  fireEvent.click(screen.getByRole("button", { name: "Show all 2 selected references" }));
  const menu = screen.getByRole("dialog", { name: "Selected references" });
  expect(within(menu).getByRole("button", { name: "Edit García 2021 · shared · First library" }).textContent).toContain("First library");
  const edit = within(menu).getByRole("button", { name: "Edit García 2021 · shared · Second library" });
  expect(document.getElementById(edit.getAttribute("aria-describedby")!)?.textContent).toBe("Second invented work");
  fireEvent.click(edit);
  fireEvent.change(screen.getByRole("textbox", { name: "Page" }), { target: { value: "42" } });
  expect(change).toHaveBeenCalledWith(second, { locator: { label: "page", value: "42", input: "page-editor" } });
});

test.each(["click", "Enter", " "])("a single reference is the whole citation segment; %s opens its menu and focuses Page", async (key) => {
  render(shelf({ selected: chosen([record("a")]), shown: output() }));
  const segment = screen.getByRole("button", { name: "Edit García 2021" });
  expect(segment.textContent).toBe("(García, 2021)");
  expect(document.getElementById(segment.getAttribute("aria-describedby")!)?.textContent).toBe("(García, 2021)");
  expect(document.querySelector(".shelf-chips")).toBeNull();
  if (key === "click") { act(() => segment.focus()); fireEvent.click(segment); } else press(segment, key);
  const menu = screen.getByRole("dialog", { name: "Citation details for Invented title a" });
  expect(menu.closest(".details-menu")).not.toBeNull();
  expect(within(menu).getByRole("heading").textContent).toBe("García 2021");
  expect(document.activeElement).toBe(within(menu).getByRole("textbox", { name: "Page" }));
  for (const name of ["Name in brackets", "Name in the sentence", "Year only"]) expect(within(menu).getByRole("radio", { name })).toBeTruthy();
  const year = within(menu).getByRole("radio", { name: "Year only" });
  expect(year.matches(":disabled, [aria-disabled=true]")).toBe(true);
  expect(document.getElementById(year.getAttribute("aria-describedby")!)?.textContent).toBe("Preparing examples…");
  expect(within(menu).getByRole("button", { name: /Remove from Citation/ })).toBeTruthy();
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  await act(async () => { await new Promise(requestAnimationFrame); });
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(document.activeElement).toBe(segment);
});

test("a pending preview can be edited before formatting finishes", () => {
  render(shelf({ selected: chosen([record("a")]), shown: null }));
  expect(screen.getByRole("status", { name: "" }).textContent).toBe("Formatting…");
  fireEvent.click(screen.getByRole("button", { name: "Edit García 2021" }));
  expect(document.activeElement).toBe(screen.getByRole("textbox", { name: "Page" }));
});

test("RTL citation text is isolated with automatic direction without changing key punctuation", () => {
  const text = "[@مثال2021, p. 12]";
  render(shelf({ selected: chosen([record("rtl")]), keys: true, style: null, shown: output(text) }));
  const citation = document.querySelector(".shelf .preview-text")!;
  expect(citation.getAttribute("dir")).toBe("auto");
  expect(citation.textContent).toBe(text);
  expect(citation.textContent).toBe(text);
  expect(citation.getAttribute("title")).toBeNull();
});

test("a long single citation can be read in full from its keyboard-opened menu without hover", () => {
  const text = "An invented long reference with its complete meaning. ".repeat(30).trim();
  render(shelf({ selected: chosen([record("a")]), shown: output(text) }));
  press(screen.getByRole("button", { name: "Edit García 2021" }), "Enter");
  const full = within(screen.getByRole("dialog")).getByRole("region", { name: "Full citation" });
  expect(full.textContent).toBe(text);
  fireEvent.keyDown(document.activeElement!, { key: "ArrowUp" });
  fireEvent.keyDown(document.activeElement!, { key: "End" });
  expect(document.activeElement).toBe(full);
});

test("pending retains dimmed old text for 300 ms, then Formatting without treating it as a new receipt", () => {
  vi.useFakeTimers();
  try {
    const reference = record("a");
    const { rerender } = render(shelf({ selected: chosen([reference]), shown: output("Original citation") }));
    rerender(shelf({ selected: chosen([{ ...reference, locator: { label: "page", value: "9" } }]), shown: null }));
    expect(document.querySelector('.shelf-output[data-state="pending"] .preview-text')?.textContent).toBe("Original citation");
    act(() => vi.advanceTimersByTime(299));
    expect(screen.queryByText("Formatting…")).toBeNull();
    act(() => vi.advanceTimersByTime(1));
    expect(screen.getByText("Formatting…")).toBeTruthy();
    expect(document.querySelector('.shelf-output[data-state="pending"] .preview-text')).toBeNull();
  } finally { cleanup(); vi.useRealTimers(); }
});

test("a later refresh of unchanged reference details gets its own 300 ms retention", () => {
  vi.useFakeTimers();
  try {
    const selected = chosen([record("a")]);
    const { rerender } = render(shelf({ selected, shown: output("Original citation") }));
    rerender(shelf({ selected, shown: null }));
    act(() => vi.advanceTimersByTime(300));
    expect(screen.getByText("Formatting…")).toBeTruthy();
    rerender(shelf({ selected, shown: output("Refreshed citation") }));
    rerender(shelf({ selected, shown: null }));
    expect(document.querySelector('.shelf-output[data-state="pending"] .preview-text')?.textContent).toBe("Refreshed citation");
    act(() => vi.advanceTimersByTime(299));
    expect(screen.queryByText("Formatting…")).toBeNull();
    act(() => vi.advanceTimersByTime(1));
    expect(screen.getByText("Formatting…")).toBeTruthy();
  } finally { cleanup(); vi.useRealTimers(); }
});

test("a formatting reply cannot retire a segment while it is being activated", () => {
  const selected = chosen([record("a")]);
  const { rerender } = render(shelf({ selected, shown: null }));
  const segment = screen.getByRole("button", { name: "Edit García 2021" });
  fireEvent.mouseDown(segment, { button: 0 });
  rerender(shelf({ selected, shown: output() }));
  fireEvent.mouseUp(segment, { button: 0 });
  fireEvent.click(segment);
  expect(document.activeElement).toBe(screen.getByRole("textbox", { name: "Page" }));
});

test("Page retains typed input, canonical pages from search, and independent reference details", () => {
  const reference: CitationReference = { ...record("a"), locator: { label: "page", value: "S.12" }, form: "year-only" };
  const change = vi.fn();
  render(shelf({ highlightedReference: reference, shown: output("(2021, S.12)"), onChangeReference: change }));
  fireEvent.click(screen.getByRole("button", { name: "Edit García 2021" }));
  const page = screen.getByRole<HTMLInputElement>("textbox", { name: "Page" });
  expect(page.value).toBe("S.12");
  fireEvent.change(page, { target: { value: "pp. ١٢–١٥" } });
  expect(change).toHaveBeenCalledWith(reference, { locator: { label: "page", value: "pp. ١٢–١٥", input: "page-editor" } });
  expect(document.activeElement).toBe(page);
  fireEvent.change(page, { target: { value: "" } });
  expect(change).toHaveBeenLastCalledWith(reference, { locator: null });
});

test.each(["Backspace", "Delete", "menu", "badge"])("%s removes exactly once; removal never opens the reference menu", (method) => {
  const reference = record("a"), remove = vi.fn();
  render(shelf({ selected: chosen([reference]), shown: output(), onRemove: remove }));
  const segment = screen.getByRole("button", { name: "Edit García 2021" });
  if (method === "menu") {
    fireEvent.click(segment);
    fireEvent.click(screen.getByRole("button", { name: /Remove from Citation/ }));
  } else if (method === "badge") fireEvent.click(screen.getByRole("button", { name: "Remove a" }));
  else press(segment, method);
  expect(remove).toHaveBeenCalledExactlyOnceWith(reference);
  expect(screen.queryByRole("dialog")).toBeNull();
});

test("hiding the output keeps the reference's one-click remove badge", () => {
  const reference = record("a"), remove = vi.fn();
  render(shelf({ selected: chosen([reference]), shown: undefined, onRemove: remove }));
  const segment = screen.getByRole("button", { name: "Edit García 2021" });
  const wrapper = segment.closest<HTMLElement>(".shelf-segment")!;
  fireEvent.click(within(wrapper).getByRole("button", { name: "Remove a" }));
  expect(remove).toHaveBeenCalledExactlyOnceWith(reference);
  expect(screen.queryByRole("dialog")).toBeNull();
});

test.each([1, 2])("many references at the minimum frame and %s× text retain exact output and one-step access to every reference", (scale) => {
  vi.stubGlobal("innerWidth", 480); vi.stubGlobal("innerHeight", 360);
  document.documentElement.style.fontSize = `${16 * scale}px`;
  const records = Array.from({ length: 50 }, (_, i) => record(`synthetic${i}`, `Author${i}`));
  const text = "An invented very long citation. ".repeat(40).trim();
  render(shelf({ selected: chosen(records), shown: output(text) }));
  const preview = screen.getByRole("region", { name: "Preview" });
  expect(preview.querySelector(".preview-text")?.textContent).toBe(text);
  const count = screen.getByRole("button", { name: "Show all 50 selected references" });
  expect(count.textContent).toBe("50");
  fireEvent.click(count);
  const menu = screen.getByRole("dialog", { name: "Selected references" });
  expect(document.querySelector(".shelf")!.contains(menu)).toBe(false);
  expect(within(menu).getAllByRole("button", { name: /^Edit Author/ })).toHaveLength(50);
  fireEvent.click(within(menu).getByRole("button", { name: "Edit Author49 2021" }));
  expect(screen.getByRole("heading").textContent).toBe("Author49 2021");
  expect(document.activeElement).toBe(screen.getByRole("textbox", { name: "Page" }));
  document.documentElement.style.fontSize = "";
});

test.each([true, false])("folded warnings and Closest matches keep a visible count and one-step access (preview=%s)", (preview) => {
  vi.stubGlobal("innerWidth", 480); vi.stubGlobal("innerHeight", 360);
  const reference = { ...record("near"), partial: true };
  const warning = "Reference 1: Check the invented year.";
  render(shelf({ selected: chosen([reference]), style: "ieee", outputWarnings: [warning], shown: preview ? output() : undefined }));
  const warnings = screen.getByRole("button", { name: /2 warnings.*Reference entry.*Closest matches/ });
  expect(warnings.textContent).toBe("! 2");
  press(warnings, " ");
  const menu = screen.getByRole("dialog", { name: "Citation warnings" });
  expect(within(menu).getByText(warning)).toBeTruthy();
  expect(within(menu).getByRole("heading", { name: "Closest matches" })).toBeTruthy();
  expect(within(menu).getByText(/only some search words matched/)).toBeTruthy();
  expect(within(menu).getByText(/IEEE numbers depend on your whole document/)).toBeTruthy();
});

test("a formatting refusal stays beside the citation before its menu opens, even with the preview hidden", () => {
  const reason = "Reference 1: This form cannot be expressed. Choose another form.";
  render(shelf({ selected: chosen([record("a")]), shown: "failed", problem: reason }));
  expect(within(screen.getByRole("region", { name: "Preview" })).getByText(reason)).toBeTruthy();
  expect(screen.queryByRole("dialog")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Details" }));
  expect(within(screen.getByRole("dialog")).getByText(reason)).toBeTruthy();
});

test("closing and reopening retires an old reference menu, focuses Search, and keeps the edited citation", async () => {
  const search = createRef<HTMLInputElement>();
  function Picker({ presentation }: { presentation: number }) {
    const [reference, setReference] = useState<CitationReference>(record("a"));
    return <><input ref={search} aria-label="Search" />{shelf({ presentation, selected: chosen([reference]), shown: output(), fallbackFocusRef: search,
      onChangeReference: (previous, update) => setReference({ ...previous, ...update }) })}</>;
  }
  const { rerender } = render(<Picker presentation={0} />);
  fireEvent.click(screen.getByRole("button", { name: "Edit García 2021" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Page" }), { target: { value: "iv–vi" } });
  rerender(<Picker presentation={1} />);
  await act(async () => { await new Promise(requestAnimationFrame); });
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(document.activeElement).toBe(screen.getByRole("textbox", { name: "Search" }));
  fireEvent.click(screen.getByRole("button", { name: "Edit García 2021" }));
  expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Page" }).value).toBe("iv–vi");
});

test("removing a focused segment sends focus to Search before it disappears", () => {
  const search = createRef<HTMLInputElement>();
  function Picker() {
    const [selected, setSelected] = useState(chosen([record("a")]));
    return <><input ref={search} aria-label="Search" />{shelf({ selected, shown: selected.size ? output() : undefined,
      fallbackFocusRef: search, onRemove: () => setSelected(new Map()) })}</>;
  }
  render(<Picker />);
  press(screen.getByRole("button", { name: "Edit García 2021" }), "Backspace");
  expect(document.activeElement).toBe(screen.getByRole("textbox", { name: "Search" }));
});

test("Copied shows the exact receipt between matching beaks, with one polite confirmation", () => {
  const text = "[@invented-first; @invented-second]";
  const { rerender } = render(shelf({ shown: output(text), copied: { text, warnings: [] } }));
  expect(screen.getByRole("status", { name: "Copied" }).textContent).toBe("Copied");
  const band = document.querySelector(".shelf")!;
  expect(band.querySelector(".preview-text")?.textContent).toBe(text);
  expect(band.querySelectorAll(".shelf-mark")).toHaveLength(2);
  expect(band.querySelector(".shelf-mark-close")?.innerHTML).toBe(band.querySelector(".shelf-mark-open")?.innerHTML);
  rerender(shelf({ shown: output("New preview") }));
  expect(screen.queryByRole("status", { name: "Copied" })).toBeNull();
  expect(document.querySelector(".preview-text")?.textContent).toBe("New preview");
});

test("a multi-reference Copied receipt has one complete read-only line for ellipsis", () => {
  const text = "(Alpha, 2021; Beta, 2021)";
  render(shelf({ selected: chosen([record("a", "Alpha"), record("b", "Beta")]), copied: { text, warnings: [] }, shown: {
    ...output(text), spans: [{ item: null, text: "(" }, { item: 0, text: "Alpha, 2021" },
      { item: null, text: "; " }, { item: 1, text: "Beta, 2021" }, { item: null, text: ")" }],
  } }));
  expect(document.querySelector('.shelf[data-copied] .preview-text')?.textContent).toBe(text);
  expect(screen.queryByRole("button", { name: /^Edit / })).toBeNull();
});

test("Copied retains the citation's scientific script and emphasis", () => {
  render(shelf({ selected: chosen([record("a")]), shown: { ...output("CO2"), markup: [
    { tag: "i", children: ["CO", { tag: "sub", children: ["2"] }] },
  ] }, copied: { text: "CO2", warnings: [] } }));
  expect(document.querySelector('.shelf[data-copied] .preview-text')?.textContent).toBe("CO2");
  expect(document.querySelector('.shelf[data-copied] i sub')?.textContent).toBe("2");
});

test("Copied warnings keep their count and one-step access; a newer receipt retires the old menu", () => {
  vi.stubGlobal("innerWidth", 480); vi.stubGlobal("innerHeight", 360);
  const copied = { text: "Copied citation", warnings: ["Check the invented year.", "Closest match: Invented title."], closestMatches: ["Closest match: Invented title."] };
  const { rerender } = render(shelf({ copied, shown: output(copied.text) }));
  const count = screen.getByRole("button", { name: "2 warnings · Closest matches" });
  expect(count.textContent).toBe("! 2");
  fireEvent.click(count);
  const menu = screen.getByRole("dialog", { name: "Citation warnings" });
  expect(within(menu).getByText("Check the invented year.")).toBeTruthy();
  expect(within(menu).getByRole("heading", { name: "Closest matches" })).toBeTruthy();
  expect(within(menu).getAllByText("Closest match: Invented title.")).toHaveLength(1);
  rerender(shelf({ copied: { text: "New citation", warnings: [] } }));
  expect(screen.queryByRole("dialog")).toBeNull();
});

test("an old receipt never replaces the live copy target or confirms a different citation", () => {
  render(shelf({ copied: { text: "Old citation", warnings: [] }, shown: output("New citation") }));
  expect(document.querySelector(".preview-text")?.textContent).toBe("New citation");
  expect(screen.queryByRole("status", { name: "Copied" })).toBeNull();
});
