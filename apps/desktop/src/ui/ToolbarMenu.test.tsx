import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import axe from "axe-core";
import { createRef } from "react";
import { ToolbarMenu, type ToolbarMenuSection } from "./ToolbarMenu";
import { setPlatform } from "../nativeAppearance";

const sections: ToolbarMenuSection[] = [{ id: "keys", label: "Key formats", items: [
  { id: "pandoc", label: "Pandoc", sample: "[@key]" },
  { id: "latex", label: "LaTeX", sample: "\\cite{key}" },
  { id: "typst", label: "Typst", sample: "@key", disabled: true },
] }];
afterEach(async () => {
  cleanup();
  await act(async () => { await new Promise(requestAnimationFrame); });
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  delete document.documentElement.dataset.platform;
  delete document.documentElement.dataset.system;
});

function mount(items = sections) {
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  Element.prototype.scrollIntoView = vi.fn();
  const onAction = vi.fn();
  const parentKeys = vi.fn();
  const returnFocusRef = createRef<HTMLInputElement>();
  render(<main onKeyDown={parentKeys}><input aria-label="Search references" ref={returnFocusRef} /><ToolbarMenu
    label="Copy as" value="Pandoc" sections={items} selectedKey="pandoc" onAction={onAction}
    filterLabel="Filter formats" returnFocusRef={returnFocusRef} /></main>);
  return { onAction, parentKeys, trigger: screen.getByRole("button", { name: "Copy as Pandoc" }), search: screen.getByRole("textbox") };
}

test.each(["macos", "windows", "linux"])("%s menu has named sections, radio items, keyboard navigation and no axe violations", async (platform) => {
  setPlatform(platform === "macos" ? "MacIntel" : platform === "windows" ? "Win32" : "Linux x86_64");
  const { trigger, search, onAction } = mount();
  act(() => trigger.focus());
  fireEvent.click(trigger);
  const menu = await screen.findByRole("menu", { name: "Copy as Pandoc" });
  expect(["menu", "true"]).toContain(trigger.getAttribute("aria-haspopup"));
  expect(trigger.getAttribute("aria-expanded")).toBe("true");
  expect(screen.getByRole("group", { name: "Key formats" })).toBeTruthy();
  expect(screen.getByRole("menuitemradio", { name: "Pandoc" }).getAttribute("aria-checked")).toBe("true");
  fireEvent.keyDown(menu, { key: "ArrowDown" });
  expect(document.activeElement).toBe(screen.getByRole("menuitemradio", { name: "LaTeX" }));
  fireEvent.keyDown(document.activeElement!, { key: "End" });
  expect(document.activeElement).toBe(screen.getByRole("menuitemradio", { name: "Typst" }));
  expect(document.activeElement?.getAttribute("aria-disabled")).toBe("true");
  fireEvent.keyDown(document.activeElement!, { key: "Enter" });
  expect(onAction).not.toHaveBeenCalled();
  expect((await axe.run(document.body, { rules: { "color-contrast": { enabled: false } } })).violations).toEqual([]);
  fireEvent.keyDown(document.activeElement!, { key: "Home" });
  fireEvent.keyDown(document.activeElement!, { key: "Enter" });
  fireEvent.keyUp(document.activeElement!, { key: "Enter" });
  await waitFor(() => expect(onAction).toHaveBeenCalledWith("pandoc"));
  await waitFor(() => expect(document.activeElement).toBe(search));
});

test("more than seven items filter with virtual focus and ignore composing shortcuts and picks", async () => {
  const { trigger, onAction } = mount([{ id: "many", label: "Formats", items: Array.from({ length: 8 }, (_, i) => ({ id: `f${i}`, label: `Format ${i}` })) }]);
  act(() => trigger.focus());
  fireEvent.click(trigger);
  const filter = await screen.findByRole("searchbox", { name: "Filter formats" });
  fireEvent.change(filter, { target: { value: "Format 7" } });
  await waitFor(() => expect(screen.getAllByRole("menuitemradio")).toHaveLength(1));
  expect(document.activeElement).toBe(filter);
  fireEvent.compositionStart(filter);
  fireEvent.keyDown(filter, { key: "Enter", isComposing: true });
  fireEvent.keyDown(filter, { key: "Escape", keyCode: 229 });
  fireEvent.keyDown(filter, { key: "k", ctrlKey: true, isComposing: true });
  expect(onAction).not.toHaveBeenCalled();
  expect(screen.getByRole("menu")).toBeTruthy();
  fireEvent.compositionEnd(filter);
  fireEvent.keyDown(filter, { key: "ArrowDown" });
  fireEvent.keyDown(filter, { key: "Enter" });
  fireEvent.keyUp(filter, { key: "Enter" });
  await waitFor(() => expect(onAction).toHaveBeenCalledWith("f7"));
});

test("keyboard opening and pointer activation keep unavailable items inert", async () => {
  const { trigger, onAction } = mount();
  act(() => trigger.focus());
  fireEvent.keyDown(trigger, { key: "ArrowDown" });
  fireEvent.keyUp(trigger, { key: "ArrowDown" });
  const unavailable = await screen.findByRole("menuitemradio", { name: "Typst" });
  fireEvent.click(unavailable);
  expect(onAction).not.toHaveBeenCalled();
  expect(screen.getByRole("menuitemradio", { name: "Pandoc" }).getAttribute("aria-checked")).toBe("true");
  expect(screen.getByRole("menu")).toBeTruthy();
});
