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

test("Esc returns the keyboard to what opened it; Tab leaves for the caller to place", () => {
  const { onClose, field } = open();
  fireEvent.keyDown(window, { key: "Escape" });
  expect(onClose).toHaveBeenCalledWith("dismiss");
  expect(document.activeElement).toBe(screen.getByRole("button", { name: "Opener" }));
  cleanup();
  const second = open();
  fireEvent.keyDown(second.field, { key: "Tab" });
  expect(second.onClose).toHaveBeenCalledWith("tab");
  void field;
});
