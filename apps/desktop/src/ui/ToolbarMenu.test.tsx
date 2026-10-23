import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import axe from "axe-core";
import { createRef, StrictMode } from "react";
import type { KeyboardEvent } from "react";
import { ToolbarMenu, type ToolbarMenuSection } from "./ToolbarMenu";

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

test.each([
  ["macos", "0 0 8 12"],
  ["windows", "0 0 12 12"],
  ["linux", "0 0 16 16"],
])("%s popup uses its bundled decorative indicator without changing the button name", async (platform, viewBox) => {
  document.documentElement.dataset.platform = platform;
  const { trigger } = mount();
  expect(trigger.classList.contains("ui-button")).toBe(true);
  expect(trigger.getAttribute("data-variant")).toBe("secondary");
  const indicator = trigger.querySelector("svg")!;
  expect(indicator.getAttribute("viewBox")).toBe(viewBox);
  expect(indicator.getAttribute("aria-hidden")).toBe("true");
  expect(indicator.getAttribute("focusable")).toBe("false");
  const drawing = indicator.outerHTML;
  fireEvent.click(trigger);
  await screen.findByRole("menu", { name: "Copy as Pandoc" });
  // React Aria hides the page outside its modal menu from accessibility.
  expect(trigger.getAttribute("aria-label")).toBe("Copy as Pandoc");
  expect(trigger.isConnected).toBe(true);
  expect(indicator.outerHTML).toBe(drawing);
});

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

test.each([false, true])("a format pick returns typing before its exit animation (filtered %s)", async (filtered) => {
  let release!: () => void;
  const finished = new Promise<void>((resolve) => { release = resolve; });
  const descriptor = Object.getOwnPropertyDescriptor(Element.prototype, "getAnimations");
  Object.defineProperty(Element.prototype, "getAnimations", { configurable: true, value(this: HTMLElement) {
    return this.hasAttribute("data-exiting") ? [{ playState: "running", finished }] : [];
  } });
  try {
    const items = filtered ? [{ id: "many", label: "Formats", items: Array.from({ length: 8 }, (_, i) => ({ id: `f${i}`, label: `Format ${i}` })) }] : sections;
    const { trigger, search, onAction } = mount(items);
    fireEvent.change(search, { target: { value: "memory" } });
    fireEvent.click(trigger);
    const menu = await screen.findByRole("menu");
    if (filtered) {
      const filter = screen.getByRole("searchbox", { name: "Filter formats" });
      fireEvent.change(filter, { target: { value: "Format 7" } });
      fireEvent.keyDown(filter, { key: "ArrowDown" });
    } else act(() => screen.getByRole("menuitemradio", { name: "Pandoc" }).focus());
    fireEvent.keyDown(document.activeElement!, { key: "Enter" });
    fireEvent.keyUp(document.activeElement!, { key: "Enter" });
    expect(onAction).toHaveBeenCalledExactlyOnceWith(filtered ? "f7" : "pandoc");
    // Do not wait for exit, animation frames or a later focus retry.
    expect(menu.isConnected).toBe(false);
    expect(document.activeElement).toBe(search);
    fireEvent.keyDown(document.activeElement!, { key: "n" });
    fireEvent.input(document.activeElement!, { target: { value: "memorynative" } });
    expect((search as HTMLInputElement).value).toBe("memorynative");
    await act(async () => { release(); await finished; await new Promise(requestAnimationFrame); });
    expect(document.activeElement).toBe(search);
    expect(onAction).toHaveBeenCalledTimes(1);
  } finally {
    await act(async () => { release(); await finished; });
    if (descriptor) Object.defineProperty(Element.prototype, "getAnimations", descriptor);
    else Reflect.deleteProperty(Element.prototype, "getAnimations");
  }
});

test("a selection callback can deliberately move focus without a later Search retry", async () => {
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  const search = createRef<HTMLInputElement>();
  const destination = createRef<HTMLButtonElement>();
  render(<><input aria-label="Search references" ref={search} /><button ref={destination}>Other action</button>
    <ToolbarMenu label="Copy as" value="Pandoc" sections={sections} selectedKey="pandoc"
      filterLabel="Filter formats" returnFocusRef={search} onAction={() => destination.current?.focus()} /></>);
  fireEvent.click(screen.getByRole("button", { name: "Copy as Pandoc" }));
  fireEvent.click(await screen.findByRole("menuitemradio", { name: "LaTeX" }));
  expect(document.activeElement).toBe(destination.current);
  await act(async () => { await new Promise(requestAnimationFrame); });
  expect(document.activeElement).toBe(destination.current);
});

function mountCommand(strict = false) {
  const searchRef = createRef<HTMLInputElement>();
  let current = true;
  const run = vi.fn(() => {
    expect(document.activeElement).toBe(searchRef.current);
    expect(screen.queryByRole("menu")).toBeNull();
  });
  const commands = { onKeyCommand: (event: KeyboardEvent) => event.ctrlKey && event.key === "k"
    ? { run, isCurrent: () => current } : undefined };
  const content = <><input aria-label="Search references" ref={searchRef} /><ToolbarMenu label="Copy as" value="Pandoc"
    sections={sections} selectedKey="pandoc" onAction={() => {}} filterLabel="Filter formats"
    returnFocusRef={searchRef} {...commands} /></>;
  render(strict ? <StrictMode>{content}</StrictMode> : content);
  fireEvent.click(screen.getByRole("button", { name: "Copy as Pandoc" }));
  return { run, invalidate: () => { current = false; }, menu: screen.getByRole("menu"), search: searchRef.current! };
}

test.each([false, true])("a key command waits for removal and Search, then runs once (StrictMode %s)", async (strict) => {
  const { menu, run } = mountCommand(strict);
  fireEvent.keyDown(menu, { key: "k", ctrlKey: true });
  expect(run).not.toHaveBeenCalled();
  fireEvent.keyUp(document.body, { key: "k", ctrlKey: true });
  await waitFor(() => expect(run).toHaveBeenCalledTimes(1));
  expect(menu.isConnected).toBe(false);
  await act(async () => { await new Promise(requestAnimationFrame); });
  expect(run).toHaveBeenCalledTimes(1);
});

test("input during Search focus cancels the consumed command", async () => {
  const { menu, search, run } = mountCommand();
  search.addEventListener("focus", () => fireEvent.keyDown(document.body, { key: "Escape" }), { once: true });
  fireEvent.keyDown(menu, { key: "k", ctrlKey: true });
  await waitFor(() => expect(menu.isConnected).toBe(false));
  await act(async () => { await new Promise(requestAnimationFrame); });
  expect(document.activeElement).toBe(search);
  expect(run).not.toHaveBeenCalled();
});

test("a command does not run when Search cannot receive focus", async () => {
  const { menu, search, run } = mountCommand();
  const focus = vi.spyOn(search, "focus").mockImplementation(() => {});
  try {
    fireEvent.keyDown(menu, { key: "k", ctrlKey: true });
    await waitFor(() => expect(menu.isConnected).toBe(false));
    await act(async () => { await new Promise(requestAnimationFrame); });
    expect(run).not.toHaveBeenCalled();
  } finally { focus.mockRestore(); }
});

test.each(["pointer", "key", "blur", "hidden"])("a later %s cancels a pending menu command", async (action) => {
  const { menu, run, invalidate } = mountCommand();
  fireEvent.keyDown(menu, { key: "k", ctrlKey: true });
  if (action === "pointer") fireEvent.pointerDown(document.body);
  else if (action === "key") fireEvent.keyDown(document.body, { key: "Escape" });
  else if (action === "blur") fireEvent(window, new Event("blur"));
  else invalidate(); // The caller's picker-hidden epoch changed.
  await waitFor(() => expect(menu.isConnected).toBe(false));
  await act(async () => { await new Promise(requestAnimationFrame); });
  expect(run).not.toHaveBeenCalled();
});

test.each(["k", "Escape", "Tab", "reopen", "unmount"])("%s during an exit stays inside the handoff", async (key) => {
  // Hold a real React Aria exit through its Web Animations API boundary.
  let release!: () => void;
  const finished = new Promise<void>((resolve) => { release = resolve; });
  const descriptor = Object.getOwnPropertyDescriptor(Element.prototype, "getAnimations");
  Object.defineProperty(Element.prototype, "getAnimations", { configurable: true, value(this: HTMLElement) {
    return this.hasAttribute("data-exiting") ? [{ playState: "running", finished }] : [];
  } });
  try {
    const { menu, run, search } = mountCommand();
    fireEvent.keyDown(menu, { key: "k", ctrlKey: true });
    expect(menu.closest(".toolbar-menu-popover")?.hasAttribute("data-exiting")).toBe(true);
    if (key === "reopen") fireEvent.click(screen.getByRole("button", { name: "Copy as Pandoc" }));
    else if (key === "unmount") cleanup();
    else fireEvent.keyDown(menu, { key, ctrlKey: key === "k" });
    expect(run).not.toHaveBeenCalled();
    await act(async () => { release(); await finished; });
    if (key === "reopen") expect(screen.getByRole("menu")).toBeTruthy();
    else {
      await waitFor(() => expect(menu.isConnected).toBe(false));
      if (key !== "unmount") await waitFor(() => expect(document.activeElement).toBe(search));
    }
    await act(async () => { await new Promise(requestAnimationFrame); });
    expect(run).toHaveBeenCalledTimes(key === "k" ? 1 : 0);
  } finally {
    release();
    if (descriptor) Object.defineProperty(Element.prototype, "getAnimations", descriptor);
    else Reflect.deleteProperty(Element.prototype, "getAnimations");
  }
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
const toolbarCss = readFileSync(new FileURL("./ToolbarMenu.css", import.meta.url), "utf8");
const menuCss = readFileSync(new FileURL("./Menu.css", import.meta.url), "utf8");
const buttonCss = readFileSync(new FileURL("./Button.css", import.meta.url), "utf8");
const tokens = readFileSync(new FileURL("../tokens.css", import.meta.url), "utf8");
test("menu role tokens exist, forced colours frame the button, and reduced motion removes movement", () => {
  const definitions = new Set([...(tokens + menuCss + toolbarCss + buttonCss).matchAll(/(--[\w-]+)\s*:/g)].map((match) => match[1]));
  expect([...(menuCss + toolbarCss + buttonCss).matchAll(/var\((--[\w-]+)/g)].map((match) => match[1]).filter((name) => !definitions.has(name))).toEqual([]);
  expect(buttonCss).toMatch(/border: var\(--border-width\) solid var\(--stroke-control\)/);
  expect(tokens).toMatch(/forced-colors: active[\s\S]*--button-edge: ButtonText/);
  expect(menuCss).toMatch(/background: Highlight; color: HighlightText/);
  expect(menuCss).toMatch(/prefers-reduced-motion: reduce[\s\S]*--menu-enter-transform: none; --menu-open-duration: 100ms/);
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

test("menu shadows stay on the menu popover while the keys dialog keeps its own", () => {
  expect(tokens).toContain("--shadow-overlay: 0 12px 32px rgb(0 0 0 / 16%);");
  expect(menuCss).toContain("box-shadow: var(--shadow-overlay);");
  expect(tokens + menuCss).not.toMatch(/--menu-shadow\s*:/);
  expect(menuCss).toMatch(/windows"\] \.menu-popover\s*\{[^}]*--menu-item-height: 32px/);
  expect(menuCss).toMatch(/linux"\] \.menu-popover\s*\{[^}]*--menu-item-height: 32px/);
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

test.each([[false, "action"], [false, "Escape"], [true, "action"], [true, "Escape"]] as const)("rapid toolbar reopening (filtered=%s, close=%s) restores its keyboard focus", async (filtered, close) => {
  const items = filtered ? [{ ...sections[0]!, items: [...sections[0]!.items,
    ...Array.from({ length: 6 }, (_, i) => ({ id: `f${i}`, label: `Format ${i}` }))] }] : sections;
  const { trigger, onAction } = mount(items);
  fireEvent.click(trigger);
  const menu = await screen.findByRole("menu");
  const popover = menu.closest(".toolbar-menu-popover")!;
  let finishExit!: () => void;
  const finished = new Promise<void>((resolve) => { finishExit = resolve; });
  Object.defineProperty(popover, "getAnimations", { value: () => popover.hasAttribute("data-exiting")
    ? [{ playState: "running", finished }] : [] });
  if (close === "action") fireEvent.click(screen.getByRole("menuitemradio", { name: "Pandoc" }));
  else fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  if (close === "action") expect(popover.isConnected).toBe(false);
  else expect(popover.hasAttribute("data-exiting")).toBe(true);
  onAction.mockClear();
  // Model the browser blurring the trigger as reopening makes the page inert.
  act(() => (document.activeElement as HTMLElement).blur());
  const focus = vi.spyOn(trigger, "focus").mockImplementation(() => {});
  try {
    fireEvent.pointerDown(trigger, { button: 0, pointerType: "mouse", pointerId: 1 });
    fireEvent.pointerUp(trigger, { button: 0, pointerType: "mouse", pointerId: 1 });
    fireEvent.click(trigger);
    await act(async () => { await new Promise(requestAnimationFrame); });
    const reopened = screen.getByRole("menu").closest(".toolbar-menu-popover");
    if (close === "action") expect(reopened).not.toBe(popover);
    else expect(reopened).toBe(popover);
    expect(screen.getByRole("menuitemradio", { name: "Pandoc" }).getAttribute("aria-checked")).toBe("true");
    if (filtered) {
      const filter = screen.getByRole("searchbox", { name: "Filter formats" });
      expect(document.activeElement).toBe(filter);
      fireEvent.change(filter, { target: { value: "LaTeX" } });
      await waitFor(() => expect(screen.getAllByRole("menuitemradio")).toHaveLength(1));
      fireEvent.keyDown(filter, { key: "ArrowDown" });
      fireEvent.keyDown(filter, { key: "Enter" });
      fireEvent.keyUp(filter, { key: "Enter" });
      await waitFor(() => expect(onAction).toHaveBeenCalledWith("latex"));
    } else {
      expect(document.activeElement).toBe(screen.getByRole("menuitemradio", { name: "Pandoc" }));
      fireEvent.keyDown(document.activeElement!, { key: "ArrowDown" });
      expect(document.activeElement).toBe(screen.getByRole("menuitemradio", { name: "LaTeX" }));
    }
  } finally {
    focus.mockRestore();
    await act(async () => { finishExit(); });
    await act(async () => { await new Promise(requestAnimationFrame); });
  }
});

test("the filter field shows its caret and no ring, except with contrast or forced colours", () => {
  expect(toolbarCss).not.toMatch(/caret-color/);
  expect(toolbarCss).toMatch(/\.toolbar-menu-filter input:focus-visible \{ outline: 0; box-shadow: none; \}/);
  expect(toolbarCss).toMatch(/prefers-contrast: more\), \(forced-colors: active\) \{\s*\.toolbar-menu-filter input:focus-visible \{ outline: var\(--focus-ring-width\)/);
});
