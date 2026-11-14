import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { Palette, type PaletteSection } from "./Palette";

afterEach(cleanup);

const sections: PaletteSection[] = [
  { id: "a", label: "Formats", items: [
    { id: "apa", label: "APA", keywords: "author date psychology", checked: true },
    { id: "ieee", label: "IEEE", disabled: true, detail: "Not available just now" },
    { id: "pandoc", label: "Pandoc", keys: "CmdOrCtrl+P" },
  ] },
  { id: "all", label: "Everything", only: "searching", items: [{ id: "typst", label: "Typst" }] },
];

function open(onAction = vi.fn(), onClose = vi.fn()) {
  render(<><button type="button">Opener</button></>);
  screen.getByRole("button", { name: "Opener" }).focus();
  render(<Palette id="test" title="Format" placeholder="Search formats" sections={sections} onAction={onAction} onClose={onClose}
    actionLabel={() => "Use"} empty="No matching formats" initial="apa" />);
  return { onAction, onClose, field: screen.getByRole("combobox", { name: "Search formats" }) };
}

test("keys move the highlight and run it; a dimmed row stays listed but never runs", () => {
  const { onAction, field } = open();
  expect(document.activeElement).toBe(field);
  const row = (name: string) => screen.getByRole("option", { name: new RegExp(`^${name}`) });
  expect(field.getAttribute("aria-activedescendant")).toBe(row("APA").id);
  expect(row("APA").getAttribute("aria-current")).toBe("true");
  fireEvent.keyDown(field, { key: "ArrowDown" });
  expect(row("IEEE").getAttribute("aria-disabled")).toBe("true");
  fireEvent.keyDown(field, { key: "Enter" });
  expect(onAction).not.toHaveBeenCalled();
  fireEvent.keyDown(field, { key: "ArrowDown" });
  fireEvent.keyDown(field, { key: "Enter" });
  expect(onAction).toHaveBeenCalledWith("pandoc");
  // One row runs once: a held Enter or a second press does nothing more.
  fireEvent.keyDown(field, { key: "Enter" });
  expect(onAction).toHaveBeenCalledOnce();
});

test("typing searches names and their synonyms, reaching rows kept for searching", () => {
  const { field } = open();
  expect(screen.queryByRole("option", { name: "Typst" })).toBeNull();
  fireEvent.change(field, { target: { value: "psychology" } });
  expect(screen.getAllByRole("option").map((option) => option.textContent)).toEqual(["APA"]);
  fireEvent.change(field, { target: { value: "typ" } });
  expect(within(screen.getByRole("group", { name: "Everything" })).getByRole("option", { name: "Typst" })).toBeTruthy();
  fireEvent.change(field, { target: { value: "zzz" } });
  expect(screen.getByText("No matching formats")).toBeTruthy();
});

test("Esc returns to its opener; Tab and Shift+Tab visit the footer in visual order", () => {
  const { onClose, field } = open();
  fireEvent.keyDown(window, { key: "Escape" });
  expect(onClose).toHaveBeenCalledWith("dismiss");
  expect(document.activeElement).toBe(screen.getByRole("button", { name: "Opener" }));
  cleanup();
  const second = open();
  fireEvent.keyDown(second.field, { key: "Tab" });
  expect(document.activeElement).toBe(screen.getByRole("button", { name: "Use" }));
  fireEvent.keyDown(document.activeElement!, { key: "Tab" });
  expect(document.activeElement).toBe(screen.getByRole("button", { name: "Back" }));
  fireEvent.keyDown(document.activeElement!, { key: "Tab" });
  expect(document.activeElement).toBe(second.field);
  fireEvent.keyDown(second.field, { key: "Tab", shiftKey: true });
  expect(document.activeElement).toBe(screen.getByRole("button", { name: "Back" }));
  expect(second.onClose).not.toHaveBeenCalled();
  void field;
});

test("a press outside the field keeps the keyboard there, so keys still drive the palette", () => {
  const { field } = open();
  expect(fireEvent.mouseDown(screen.getByText("Formats"))).toBe(false);
  fireEvent.keyDown(screen.getByRole("dialog"), { key: "ArrowDown" });
  expect(field.getAttribute("aria-activedescendant")).toBe(screen.getByRole("option", { name: /^IEEE/ }).id);
});

test("the search names the view visually, while the dialog keeps its name and Back has no native tooltip", () => {
  open();
  const dialog = screen.getByRole("dialog", { name: "Format" });
  expect(screen.queryByRole("heading", { name: "Format" })).toBeNull();
  const back = dialog.querySelector(".ui-view-back")!;
  expect(back.getAttribute("aria-label")).toBe("Back");
  expect(back.hasAttribute("title")).toBe(false);
});

test("hovering the last visible row preserves scroll; rows sliding under a still pointer preserve the highlight; arrows reveal it", () => {
  render(<Palette id="scroll" title="Format" placeholder="Search formats"
    sections={[{ id: "all", items: Array.from({ length: 30 }, (_, index) => ({ id: String(index), label: `Format ${index}` })) }]}
    onAction={vi.fn()} onClose={vi.fn()} actionLabel={() => "Use"} empty="No formats" />);
  const field = screen.getByRole("combobox"), list = screen.getByRole("listbox"), rows = screen.getAllByRole("option");
  // A 100px viewport shows two whole 40px rows and part of the third. Model
  // nearest scrolling so an unwanted reveal really changes the visible place.
  rows.forEach((row, index) => Object.defineProperty(row, "scrollIntoView", { value: () => {
    const top = index * 40, bottom = top + 40;
    if (top < list.scrollTop) list.scrollTop = top;
    else if (bottom > list.scrollTop + 100) list.scrollTop = bottom - 100;
  } }));
  // A scroll gesture can arrive before the first pointer move in the view.
  fireEvent.wheel(rows[2]!, { clientX: 29, clientY: 90, deltaY: 1 });
  fireEvent.pointerMove(rows[2]!, { pointerType: "mouse", clientX: 29, clientY: 90 });
  expect(field.getAttribute("aria-activedescendant")).toBe(rows[0]!.id);
  fireEvent.pointerMove(rows[2]!, { pointerType: "mouse", clientX: 30, clientY: 90 });
  expect(field.getAttribute("aria-activedescendant")).toBe(rows[2]!.id);
  expect(list.scrollTop).toBe(0);
  list.scrollTop = 100; fireEvent.scroll(list);
  fireEvent.pointerMove(rows[4]!, { pointerType: "mouse", clientX: 30, clientY: 90 });
  expect(field.getAttribute("aria-activedescendant")).toBe(rows[2]!.id);
  fireEvent.pointerMove(rows[4]!, { pointerType: "mouse", clientX: 31, clientY: 90 });
  expect(field.getAttribute("aria-activedescendant")).toBe(rows[4]!.id);
  expect(list.scrollTop).toBe(100);
  fireEvent.pointerMove(rows[5]!, { pointerType: "touch", clientX: 32, clientY: 90 });
  expect(field.getAttribute("aria-activedescendant")).toBe(rows[4]!.id);
  fireEvent.keyDown(field, { key: "ArrowDown" });
  expect(field.getAttribute("aria-activedescendant")).toBe(rows[5]!.id);
  expect(list.scrollTop).toBe(140);
  fireEvent.keyDown(field, { key: "ArrowUp" });
  expect(field.getAttribute("aria-activedescendant")).toBe(rows[4]!.id);
  expect(list.scrollTop).toBe(140);
  fireEvent.keyDown(field, { key: "End" });
  expect(list.scrollTop).toBe(1100);
  list.scrollTop = 0; fireEvent.scroll(list);
  // A key still reveals the current row when it cannot move any farther.
  fireEvent.keyDown(field, { key: "ArrowDown" });
  expect(list.scrollTop).toBe(1100);
});

test("Backspace in an empty search goes back like Esc; while there is text, or held down after clearing it, it only edits", () => {
  const { onClose, field } = open();
  fireEvent.change(field, { target: { value: "ap" } });
  fireEvent.keyDown(field, { key: "Backspace" });
  expect(onClose).not.toHaveBeenCalled();
  fireEvent.change(field, { target: { value: "" } });
  fireEvent.keyDown(field, { key: "Backspace", repeat: true });
  expect(onClose).not.toHaveBeenCalled();
  fireEvent.keyUp(field, { key: "Backspace" });
  fireEvent.keyDown(field, { key: "Backspace" });
  expect(onClose).toHaveBeenCalledWith("dismiss");
  expect(document.activeElement).toBe(screen.getByRole("button", { name: "Opener" }));
});
