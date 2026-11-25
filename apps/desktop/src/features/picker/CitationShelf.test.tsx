import { recordId } from "../../test-fixtures";
import { displayTree } from "../../testing/app";
import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import type { SearchResult } from "../../api";
import type { CitationReference } from "./choice";
import { CitationShelf, type CitationLineActions } from "./CitationShelf";
import { useCitationInspector } from "./useCitationInspector";

afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

function record(key: string, first_author = "García", year: string | null = "2021"): SearchResult {
  return { revision: "1", id: { library: "invented.bib", key: recordId(key) }, key, cite: "1", source: "invented.bib", title: `Invented title ${key}`, authors: first_author,
    year, matched: ["title"], partial: false, first_author, author_count: 1, container: null, item_type: "book" };
}
const chosen = (records: CitationReference[]) => new Map(records.map((r) => [r.key, r]));
const output = (text = "(García, 2021)") => ({ display: displayTree([{ item: 0, text }]), text, warnings: [] });
type ShelfProps = Partial<Parameters<typeof CitationShelf>[0]>;
function Shelf(props: ShelfProps) {
  const inspector = useCitationInspector();
  return <CitationShelf inspector={inspector} announce={false} selected={new Map()}
    onRemove={() => undefined} onChangeReference={() => undefined}
    style="apa" keys={false} shown={undefined} focusable={false} outputWarnings={[]} {...props} />;
}
const shelf = (props: ShelfProps) => <Shelf {...props} />;
const details = (name: RegExp | string) => screen.getByRole("dialog", { name });
const press = (node: HTMLElement, key: string) => {
  act(() => node.focus());
  fireEvent.keyDown(node, { key, code: key === " " ? "Space" : key });
  fireEvent.keyUp(node, { key, code: key === " " ? "Space" : key });
};

test("engine-owned tokens retain punctuation, per-reference removal and arrow selection", () => {
  const a = record("a", "Alpha"), b = record("b", "Beta"), remove = vi.fn();
  const formatted = { ...output("(Beta, 2021; Alpha, 2021)"), display: displayTree([
    { item: null, text: "(" }, { item: 1, text: "Beta, 2021" }, { item: null, text: "; " },
    { item: 0, text: "Alpha, 2021" }, { item: null, text: ")" },
  ]) };
  render(shelf({ selected: chosen([a, b]), shown: formatted, onRemove: remove }));
  const beta = screen.getByRole("button", { name: "Edit Beta 2021" });
  const alpha = screen.getByRole("button", { name: "Edit Alpha 2021" });
  expect(document.querySelector(".shelf-citation")?.textContent).toBe(formatted.text);
  act(() => beta.focus());
  fireEvent.keyDown(beta, { key: "ArrowRight" });
  expect(document.activeElement).toBe(alpha);
  fireEvent.keyDown(alpha, { key: "Delete" });
  expect(remove).toHaveBeenCalledExactlyOnceWith(a);
});

test("formatted tokens preserve engine emphasis and keep source strings inert", () => {
  const a = record("a", "Alpha"), b = record("b", "Beta");
  const text = "Alpha species; Beta <script>literal</script>";
  render(shelf({ selected: chosen([a, b]), shown: { ...output(text), display: displayTree([
    { item: 0, text: "Alpha species", runs: [{ text: "Alpha species", italic: true, bold: false, script: "normal" }] },
    { item: null, text: "; " },
    { item: 1, text: "Beta <script>literal</script>", runs: [{ text: "Beta <script>literal</script>", italic: false, bold: true, script: "normal" }] },
  ]) } }));
  expect(document.querySelector(".shelf-citation")?.textContent).toBe(text);
  expect(document.querySelector(".shelf-citation i")?.textContent).toBe("Alpha species");
  expect(document.querySelector(".shelf-citation b")?.textContent).toBe("Beta <script>literal</script>");
  expect(document.querySelector(".shelf script")).toBeNull();
});

test.each(["click", "ArrowDown", " "])("a single reference is the whole citation token; %s opens its details and focuses Page", (key) => {
  render(shelf({ selected: chosen([record("a")]), shown: output() }));
  const token = screen.getByRole("button", { name: "Edit García 2021" });
  expect(token.textContent).toBe("(García, 2021)");
  expect(document.getElementById(token.getAttribute("aria-describedby")!)?.textContent).toBe("(García, 2021)");
  expect(document.querySelector(".shelf-chips")).toBeNull();
  if (key === "click") { act(() => token.focus()); fireEvent.click(token); } else if (key === "ArrowDown") { act(() => token.focus()); fireEvent.keyDown(token, { key }); } else press(token, key);
  const inspector = details("Citation details for Invented title a");
  // The full title, said once: no second heading repeats it below.
  expect(within(inspector).getByRole("heading", { level: 2 }).textContent).toBe("Invented title a");
  expect(within(inspector).queryAllByRole("heading", { level: 3 })).toHaveLength(0);
  expect(document.activeElement).toBe(within(inspector).getByRole("textbox", { name: "Page" }));
  // Pressed again, the token closes its details and takes focus back.
  fireEvent.click(token);
  expect(screen.queryByRole("dialog", { name: /^Citation details for/ })).toBeNull();
  expect(document.activeElement).toBe(token);
});

test("Enter on a token pastes, as it does everywhere, and never opens its details", () => {
  const submit = vi.fn();
  render(shelf({ selected: chosen([record("a")]), shown: output(), onSubmit: submit }));
  press(screen.getByRole("button", { name: "Edit García 2021" }), "Enter");
  expect(submit).toHaveBeenCalledOnce();
  expect(screen.queryByRole("dialog", { name: /^Citation details for/ })).toBeNull();
});

const removeOnce = (method: string) => {
  const reference = record("a"), remove = vi.fn();
  render(shelf({ selected: chosen([reference]), shown: output(), onRemove: remove }));
  const token = screen.getByRole("button", { name: "Edit García 2021" });
  if (method === "×") fireEvent.click(screen.getByRole("button", { name: "Remove García 2021" }));
  else press(token, method);
  expect(remove).toHaveBeenCalledExactlyOnceWith(reference);
  expect(screen.queryByRole("dialog", { name: /^Citation details for/ })).toBeNull();
};
test.each(["Backspace", "Delete", "×"])("%s removes a token exactly once and never opens the details", removeOnce);

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

test("More's Move Left and Right go the way the keys do in a right-to-left citation", () => {
  // Sol's probe: on the last of three Arabic tokens Shift+→ steps -1, so More's
  // Move Right must too, and interior tokens follow their label.
  const records = [record("a", "حداد"), record("b", "سالم"), record("c", "نصار")];
  const formatted = { ...output("(حداد، 2021؛ سالم، 2021؛ نصار، 2021)"), display: displayTree([
    { item: null, text: "(" }, { item: 0, text: "حداد، 2021" }, { item: null, text: "؛ " },
    { item: 1, text: "سالم، 2021" }, { item: null, text: "؛ " }, { item: 2, text: "نصار، 2021" }, { item: null, text: ")" },
  ]) };
  const computed = window.getComputedStyle;
  vi.spyOn(window, "getComputedStyle").mockImplementation((node, pseudo) => {
    const style = computed(node, pseudo);
    return node instanceof Element && node.closest(".shelf-fragments") ? { ...style, direction: "rtl", getPropertyValue: style.getPropertyValue.bind(style) } as CSSStyleDeclaration : style;
  });
  const moved = vi.fn(), actions = { current: null as CitationLineActions | null };
  render(shelf({ selected: chosen(records), shown: formatted, keepOrder: true, actionsRef: actions, onMoveReference: (reference, _order, step) => moved(reference.key, step) }));
  const last = screen.getByRole("button", { name: /^Edit نصار/ });
  act(() => last.focus());
  fireEvent.keyDown(last, { key: "ArrowRight", shiftKey: true });
  act(() => last.focus());
  act(() => actions.current!.move(1));
  expect(moved.mock.calls).toEqual([["c", -1], ["c", -1]]);
  moved.mockClear();
  const middle = screen.getByRole("button", { name: /^Edit سالم/ });
  act(() => middle.focus());
  act(() => actions.current!.move(-1));
  act(() => middle.focus());
  fireEvent.keyDown(middle, { key: "ArrowLeft", shiftKey: true });
  expect(moved.mock.calls).toEqual([["b", 1], ["b", 1]]);
});
