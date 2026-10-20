import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import axe from "axe-core";
import { createRef } from "react";
import { ToolbarMenu, type ToolbarMenuSection } from "./ToolbarMenu";

const sections: ToolbarMenuSection[] = [{ id: "keys", label: "Key formats", items: [
  { id: "pandoc", label: "Pandoc", sample: "[@key]" },
  { id: "latex", label: "LaTeX", sample: "\\cite{key}" },
  { id: "typst", label: "Typst", sample: "@key", disabled: true },
] }];
afterEach(() => { cleanup(); vi.unstubAllGlobals(); delete document.documentElement.dataset.platform; });

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
  document.documentElement.dataset.platform = platform;
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

test.each(["Escape", "Tab"])("%s closes just the menu and returns focus", async (key) => {
  const { trigger, search, parentKeys } = mount();
  act(() => trigger.focus());
  fireEvent.click(trigger);
  const menu = await screen.findByRole("menu");
  fireEvent.keyDown(menu, { key });
  await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
  await waitFor(() => expect(document.activeElement).toBe(search));
  expect(parentKeys).not.toHaveBeenCalled();
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

const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
const menuCss = readFileSync(new FileURL("./ToolbarMenu.css", import.meta.url), "utf8");
const tokens = readFileSync(new FileURL("../tokens.css", import.meta.url), "utf8");
test("menu role tokens exist, forced colours frame the button, and reduced motion removes movement", () => {
  const definitions = new Set([...tokens.matchAll(/(--[\w-]+)\s*:/g)].map((match) => match[1]));
  expect([...menuCss.matchAll(/var\((--[\w-]+)/g)].map((match) => match[1]).filter((name) => !definitions.has(name))).toEqual([]);
  expect(menuCss).toMatch(/forced-colors: active[\s\S]*border: 1px solid ButtonText/);
  expect(menuCss).toMatch(/background: Highlight; color: HighlightText/);
  expect(tokens).toMatch(/prefers-reduced-motion: reduce[\s\S]*--menu-enter-transform: none; --menu-open-duration: 100ms/);
  expect(menuCss).toMatch(/prefers-reduced-motion: reduce[\s\S]*transition: none/);
});

test("Space on an unavailable choice neither selects nor closes", async () => {
  const { trigger, onAction } = mount();
  fireEvent.click(trigger);
  const menu = await screen.findByRole("menu");
  fireEvent.keyDown(menu, { key: "ArrowDown" });
  fireEvent.keyDown(document.activeElement!, { key: "End" });
  const unavailable = screen.getByRole("menuitemradio", { name: "Typst" });
  expect(document.activeElement).toBe(unavailable);
  fireEvent.keyDown(unavailable, { key: " " });
  fireEvent.keyUp(unavailable, { key: " " });
  expect(onAction).not.toHaveBeenCalled();
  expect(screen.getByRole("menuitemradio", { name: "Pandoc" }).getAttribute("aria-checked")).toBe("true");
  expect(screen.getByRole("menu")).toBeTruthy();
});

test("an empty filter keeps the menu open and Escape still contains the layer", async () => {
  const { trigger, search } = mount([{ id: "many", label: "Formats", items: Array.from({ length: 8 }, (_, i) => ({ id: `f${i}`, label: `Format ${i}` })) }]);
  fireEvent.click(trigger);
  const filter = await screen.findByRole("searchbox");
  fireEvent.change(filter, { target: { value: "no matches 研究" } });
  await waitFor(() => expect(screen.queryAllByRole("menuitemradio")).toHaveLength(0));
  expect(screen.getByRole("menu")).toBeTruthy();
  fireEvent.keyDown(filter, { key: "Escape" });
  await waitFor(() => expect(document.activeElement).toBe(search));
  expect(screen.queryByRole("menu")).toBeNull();
});

test("toolbar shadows stay local while legacy More and keys shadows retain their values", () => {
  const legacy = tokens.slice(0, tokens.indexOf("/* ADR 0021 look layer"));
  expect(legacy).toContain("--menu-shadow: 0 .5rem 1.5rem rgb(0 0 0 / 20%);");
  expect(legacy).toContain("--menu-shadow: 0 1rem 2.5rem rgb(0 0 0 / 22%);");
  const look = tokens.slice(legacy.length);
  for (const [, selector] of look.matchAll(/([^{}]+)\{[^{}]*--menu-shadow:/g)) expect(selector).toContain(".toolbar-menu-popover");
  expect(look).toMatch(/windows"\] \.toolbar-menu-popover\s*\{[^}]*--menu-item-height: 32px/);
  expect(look).toMatch(/linux"\] \.toolbar-menu-popover\s*\{[^}]*--menu-item-height: 32px/);
});

test.each(["ctrlKey", "metaKey"])("%s shortcuts reach the picker only outside composition", async (modifier) => {
  const { trigger, parentKeys } = mount();
  fireEvent.click(trigger);
  const menu = await screen.findByRole("menu");
  fireEvent.keyDown(menu, { key: "k", [modifier]: true });
  expect(parentKeys).toHaveBeenCalledOnce();
  parentKeys.mockClear();
  fireEvent.compositionStart(menu);
  fireEvent.keyDown(menu, { key: "k", [modifier]: true });
  fireEvent.keyDown(menu, { key: "Escape" });
  expect(parentKeys).not.toHaveBeenCalled();
  expect(screen.getByRole("menu")).toBeTruthy();
  fireEvent.compositionEnd(menu);
});

test("Escape on a closed trigger reaches the picker, except during composition", () => {
  const { trigger, parentKeys } = mount();
  fireEvent.keyDown(trigger, { key: "Escape" });
  expect(parentKeys).toHaveBeenCalledOnce();
  parentKeys.mockClear();
  fireEvent.compositionStart(trigger);
  fireEvent.keyDown(trigger, { key: "Escape" });
  expect(parentKeys).not.toHaveBeenCalled();
  fireEvent.compositionEnd(trigger);
});
