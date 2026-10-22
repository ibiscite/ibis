import { afterEach, expect, test, vi } from "vitest";
import { cleanup, createEvent, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { App } from "./App";

// Vitest can load these existing runtime builtins without a Node-types dependency.
const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
const styles = readFileSync(new FileURL("./styles.css", import.meta.url), "utf8");

const originalScroll = Object.getOwnPropertyDescriptor(Element.prototype, "scrollIntoView");

afterEach(async () => {
  if (originalScroll) Object.defineProperty(Element.prototype, "scrollIntoView", originalScroll);
  else Reflect.deleteProperty(Element.prototype, "scrollIntoView");
  cleanup();
  await new Promise((resolve) => setTimeout(resolve, 0));
  clearMocks();
});

function result(key: string, title: string) {
  return { revision: "1", id: { source: "refs.bib", native_key: key }, key, title, authors: "Smith", year: 2020, matched: ["title"], first_author: "Smith", author_count: 1 };
}

function backend(search: (query: string) => ReturnType<typeof result>[]) {
  mockIPC((cmd, args) => {
    const handlers: Record<string, () => unknown> = {
      shortcut_status: () => ({ shortcut: "Ctrl+Shift+Y", registered: true, error: null }),
      search: () => ({ revision: "1", results: search(String((args as { query?: string } | undefined)?.query ?? "")) }),
      hide_picker: () => null,
      set_pinned: () => null,
      copy_format: () => "pandoc",
      picker_preferences: () => ({ citation_style: null, density: "comfortable", larger_text: false, show_paste_preview: true }),
      set_picker_preferences: () => null,
      library_status: () => ({ revision: "1", state: "loaded", has_snapshot: true, zotero: false, name: "refs.bib", snapshot_name: "refs.bib", references: 3, warnings: 0, error: null, note: null }),
    };
    const handler = handlers[cmd];
    if (!handler) throw new Error(`unexpected command ${cmd}`);
    return handler();
  }, { shouldMockEvents: true });
}

const three = [result("a", "Alpha"), result("b", "Beta"), result("c", "Gamma")];

const row = (title: string) => screen.getByText(title).closest('[role="option"]') as HTMLElement;
const activeTitle = () => within(screen.getByRole("listbox", { name: "Results" })).getByRole("option", { selected: true }).querySelector(".result-title")?.textContent;

/** A mouse move at a screen position, optionally at a given event time. */
function move(target: Element, x: number, y: number, at?: number) {
  const event = createEvent.mouseMove(target, { screenX: x, screenY: y, bubbles: true });
  if (at !== undefined) Object.defineProperty(event, "timeStamp", { value: at });
  fireEvent(target, event);
}

test("moving the mouse over a row makes it the active row", async () => {
  backend(() => three);
  render(<App />);
  await screen.findByText("Gamma");
  expect(activeTitle()).toBe("Alpha");
  move(row("Alpha"), 10, 10);
  move(row("Beta"), 10, 40);
  expect(activeTitle()).toBe("Beta");
  move(row("Gamma"), 10, 70);
  expect(activeTitle()).toBe("Gamma");
});

test("a row sliding under a still pointer while typing does not change the selection", async () => {
  backend((query) => (query ? [result("b", "Beta"), result("c", "Gamma")] : three));
  render(<App />);
  await screen.findByText("Gamma");
  move(row("Alpha"), 10, 10);
  move(row("Alpha"), 11, 10);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "e" } });
  await waitFor(() => expect(screen.queryByText("Alpha")).toBeNull());
  expect(activeTitle()).toBe("Beta");
  // The browser repeats the last mouse position over the row now beneath it.
  move(row("Gamma"), 11, 10);
  expect(activeTitle()).toBe("Beta");
});

test("the wheel scrolls without changing the selection", async () => {
  backend(() => three);
  render(<App />);
  await screen.findByText("Gamma");
  move(row("Alpha"), 10, 10);
  move(row("Alpha"), 10, 11);
  const list = screen.getByRole("listbox", { name: "Results" });
  fireEvent.wheel(row("Alpha"), { deltaY: 120 });
  fireEvent.scroll(list, { target: { scrollTop: 40 } });
  move(row("Beta"), 10, 11);
  expect(activeTitle()).toBe("Alpha");
  move(row("Beta"), 10, 14);
  expect(activeTitle()).toBe("Beta");
});

test("the first move after the pointer has been still does not select", async () => {
  backend(() => three);
  render(<App />);
  await screen.findByText("Gamma");
  move(row("Alpha"), 10, 10, 1000);
  // The picker reopened under a pointer that moved while it was hidden.
  move(row("Gamma"), 10, 300, 60_000);
  expect(activeTitle()).toBe("Alpha");
  move(row("Gamma"), 10, 302, 60_016);
  expect(activeTitle()).toBe("Gamma");
});

test("selecting by pointer does not scroll the list, but arrow keys do", async () => {
  backend(() => three);
  const scrolled = vi.fn();
  Element.prototype.scrollIntoView = scrolled;
  render(<App />);
  await screen.findByText("Gamma");
  scrolled.mockClear();
  move(row("Alpha"), 10, 10);
  move(row("Gamma"), 10, 70);
  expect(activeTitle()).toBe("Gamma");
  expect(scrolled).not.toHaveBeenCalled();
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "ArrowUp" });
  expect(activeTitle()).toBe("Beta");
  expect(scrolled).toHaveBeenCalledTimes(1);
});

test("hovering a row only tints it, swaps the tile's icon for its add control, and never changes its layout", () => {
  const hoverRules = [...styles.matchAll(/([^{}]*\[role="option"\][^{}]*:hover[^{}]*)\{([^}]*)\}/g)];
  expect(hoverRules.length).toBeGreaterThan(0);
  for (const [, , body] of hoverRules) {
    const properties = (body ?? "").split(";").map((d) => d.split(":")[0]?.trim()).filter(Boolean);
    for (const property of properties) expect(["background", "background-color", "color", "opacity"]).toContain(property);
  }
});

test("clicking a row's tile adds or removes it without copying, and keeps the search focused", async () => {
  backend(() => three);
  render(<App />);
  await screen.findByText("Gamma");
  const tile = (title: string) => row(title).querySelector<HTMLElement>(".tile-button")!;
  expect(tile("Beta").title).toBe("Add Smith 2020 to citation");
  fireEvent.click(tile("Beta"));
  await waitFor(() => expect(row("Beta").hasAttribute("data-citation-selected")).toBe(true));
  expect(activeTitle()).toBe("Beta");
  expect(document.activeElement).toBe(screen.getByLabelText("Search references"));
  expect(screen.queryByText(/Copied/)).toBeNull();
  expect(tile("Beta").title).toBe("Remove Smith 2020 from citation");
  fireEvent.click(tile("Beta"));
  await waitFor(() => expect(row("Beta").hasAttribute("data-citation-selected")).toBe(false));
  // A result from an older payload without a type still draws an icon (other).
  expect(row("Alpha").querySelector(".type-icon")).not.toBeNull();
});
