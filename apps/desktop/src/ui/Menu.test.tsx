import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import axe from "axe-core";
import { createRef, useRef, useState } from "react";
import { Menu, MenuDetail, MenuItem, MenuSection, MenuSeparator, menuLeast } from "./Menu";
import { fitSide, Region } from "./region";
import { ToolbarMenu } from "./ToolbarMenu";

afterEach(async () => {
  cleanup();
  // React Aria restores focus on a frame after unmount; finish it before the
  // next test mounts a new scope.
  await act(async () => { await new Promise(requestAnimationFrame); });
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  delete document.documentElement.dataset.platform;
});

function Harness({ onAction, toggle }: { onAction: (id: string) => void; toggle: (id: string) => void }) {
  const [open, setOpen] = useState(false);
  const [on, setOn] = useState(new Set(["first"]));
  const search = useRef<HTMLInputElement>(null);
  const flip = (id: string) => { toggle(id); setOn((now) => { const next = new Set(now); if (next.has(id)) next.delete(id); else next.add(id); return next; }); };
  return <main onKeyDown={(event) => { if (event.key === "k" && event.ctrlKey) setOpen((now) => !now); }}>
    <input ref={search} aria-label="Search" />
    <Menu label="Actions" trigger="Actions" isOpen={open} onOpenChange={setOpen} tabFocusRef={search}>
      <MenuSection id="plain" label="Last outcome. Copied. Pasting is off" header={<MenuDetail title="Copied. Pasting is off" />}>
        <MenuItem id="open" label="Open" keys="Ctrl+O" onAction={() => { onAction("open"); setOpen(false); search.current?.focus(); }} />
      </MenuSection>
      <MenuSeparator />
      <MenuSection id="toggles" label="Sources">
        <MenuItem id="first" label="First" checked={on.has("first")} disabled={on.size === 1 && on.has("first")} onAction={() => flip("first")} />
        <MenuItem id="second" label="Second" checked={on.has("second")} onAction={() => flip("second")} />
      </MenuSection>
    </Menu>
  </main>;
}

function mount() {
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  Element.prototype.scrollIntoView = vi.fn();
  const onAction = vi.fn();
  const toggle = vi.fn();
  render(<Harness onAction={onAction} toggle={toggle} />);
  return { onAction, toggle, trigger: screen.getByRole("button", { name: "Actions" }), search: screen.getByRole<HTMLInputElement>("textbox") };
}

test.each(["macos", "windows", "linux"])("%s: a pointer opens with nothing highlighted, a shortcut highlights the first item, axe passes", async (platform) => {
  document.documentElement.dataset.platform = platform;
  const { trigger, search } = mount();
  fireEvent.pointerDown(trigger, { button: 0, pointerType: "mouse" });
  const menu = await screen.findByRole("menu", { name: "Actions" });
  expect(screen.getByRole("group", { name: "Last outcome. Copied. Pasting is off" }).textContent).toContain("Copied. Pasting is off");
  expect(document.activeElement).toBe(menu);
  expect(menu.querySelector("[data-focused]")).toBeNull();
  fireEvent.keyDown(menu, { key: "ArrowDown" });
  expect(document.activeElement).toBe(screen.getByRole("menuitem", { name: "Open" }));
  const canvas = vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
  try {
    const found = await axe.run(document.body, { runOnly: { type: "tag", values: ["wcag2a", "wcag2aa"] }, rules: { "color-contrast": { enabled: false } } });
    expect(found.violations.map((v) => v.id)).toEqual([]);
  } finally {
    canvas.mockRestore();
  }
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  expect(screen.queryByRole("menu")).toBeNull();
  await waitFor(() => expect(document.activeElement).toBe(trigger));

  act(() => search.focus());
  fireEvent.keyDown(search, { key: "k", ctrlKey: true });
  await screen.findByRole("menu");
  expect(document.activeElement).toBe(screen.getByRole("menuitem", { name: "Open" }));
});

test("the last checked item stays focusable and unavailable; toggles keep the menu open; Tab goes to the search box", async () => {
  const { trigger, search, toggle, onAction } = mount();
  fireEvent.click(trigger);
  const menu = await screen.findByRole("menu");
  act(() => menu.focus());
  const first = screen.getByRole("menuitemcheckbox", { name: "First" });
  expect(first.getAttribute("aria-disabled")).toBe("true");
  fireEvent.keyDown(document.activeElement!, { key: "End" });
  fireEvent.keyDown(document.activeElement!, { key: "ArrowUp" });
  expect(document.activeElement).toBe(first);
  fireEvent.click(first);
  expect(toggle).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("menuitemcheckbox", { name: "Second" }));
  expect(toggle).toHaveBeenCalledWith("second");
  expect(screen.getByRole("menuitemcheckbox", { name: "Second" }).getAttribute("aria-checked")).toBe("true");
  expect(screen.getByRole("menuitemcheckbox", { name: "First" }).getAttribute("aria-disabled")).toBeNull();
  fireEvent.keyDown(document.activeElement!, { key: "Tab" });
  expect(screen.queryByRole("menu")).toBeNull();
  await waitFor(() => expect(document.activeElement).toBe(search));
  expect(onAction).not.toHaveBeenCalled();
});

function holdExit(menu: HTMLElement) {
  let finishExit!: () => void;
  const finished = new Promise<void>((resolve) => { finishExit = resolve; });
  const popover = menu.closest(".menu-popover")!;
  // JSDOM has no CSS animation lifecycle: hold the real popover mounted, as
  // React Aria does while any exit animation runs.
  Object.defineProperty(popover, "getAnimations", { value: () => popover.hasAttribute("data-exiting")
    ? [{ playState: "running", finished }] : [] });
  return { popover, finish: async () => {
    await act(async () => { finishExit(); });
    await act(async () => { await new Promise(requestAnimationFrame); });
  } };
}

test.each(["Escape", "Tab"])("%s closes at once and returns focus, with no exit to wait for", async (key) => {
  const { trigger, search } = mount();
  act(() => trigger.focus());
  fireEvent.click(trigger);
  const menu = await screen.findByRole("menu");
  const exit = holdExit(menu);
  act(() => menu.focus());
  fireEvent.keyDown(menu, { key });
  expect(exit.popover.isConnected).toBe(false);
  expect(document.activeElement).toBe(key === "Escape" ? trigger : search);
  await exit.finish();
});

test.each(["Escape", "Tab", "Enter", " "])("composing %s does not dismiss or activate a menu", async (key) => {
  const { trigger, onAction, toggle } = mount();
  fireEvent.keyDown(trigger, { key: "ArrowDown" });
  const menu = await screen.findByRole("menu");
  for (const composition of [{ isComposing: true }, { keyCode: 229 }]) {
    fireEvent.keyDown(document.activeElement!, { key, ...composition });
    fireEvent.keyUp(document.activeElement!, { key, ...composition });
    expect(screen.getByRole("menu")).toBe(menu);
    expect(onAction).not.toHaveBeenCalled();
    expect(toggle).not.toHaveBeenCalled();
  }
  fireEvent.keyDown(document.activeElement!, { key: "Enter" });
  fireEvent.keyUp(document.activeElement!, { key: "Enter" });
  expect(onAction).toHaveBeenCalledWith("open");
});

// Decision 0062: what floats fits its region. jsdom lays nothing out, so the
// window, the region and the rows report invented boxes.
const box = (top: number, height: number, left = 0, width = 720) => new DOMRect(left, top, width, height);

test("the side decision opens where the least fits, keeps to the region first, and has no answer when neither side has room", () => {
  const fit = (anchorTop: number, least: number, boundaries = [box(0, 440), box(0, 480)]) =>
    fitSide({ anchor: box(anchorTop, 24), boundaries, least, preferred: "bottom", gap: 4, padding: 8 });
  // Room below: the preferred side.
  expect(fit(60, 132)).toEqual({ side: "bottom", boundary: box(0, 440) });
  // A field low in the drawer, just above the bar: it opens above, inside the region.
  expect(fit(380, 132)).toEqual({ side: "top", boundary: box(0, 440) });
  // Only the window has room below (a short window): the window is the boundary.
  expect(fit(20, 120, [box(0, 150), box(0, 240)])).toEqual({ side: "bottom", boundary: box(0, 240) });
  // Neither side of any boundary: the owner opens it as a view, or takes the larger side.
  expect(fit(100, 200, [box(0, 200), box(0, 240)])).toBeNull();
  // Zero matches: the least is the filter and its empty state, which fits anywhere.
  expect(fit(360, 44)).toEqual({ side: "bottom", boundary: box(0, 440) });
});

/** A ToolbarMenu of `count` rows, 28 px each, in a region the size of a
 * `height`-tall window, its trigger at `top`; the return field sits elsewhere. */
function fitted(top: () => number, height: () => number, count = 29, selected = "term-0") {
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  Element.prototype.scrollIntoView = vi.fn();
  vi.spyOn(document.documentElement, "clientHeight", "get").mockImplementation(() => height());
  vi.spyOn(document.documentElement, "clientWidth", "get").mockReturnValue(720);
  const region = document.createElement("div");
  document.body.append(region);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this === region) return box(0, height());
    if (this.matches(".toolbar-menu-button")) return box(top(), 24, 100, 80);
    if (this.matches('[role="menu"]')) return box(0, 0);
    const rows = [...this.closest('[role="menu"]')?.querySelectorAll('[role^="menuitem"]') ?? []];
    if (rows.includes(this)) return box(rows.indexOf(this) * 28, 28);
    return new DOMRect();
  });
  const field = createRef<HTMLInputElement>();
  const onAction = vi.fn();
  const items = Array.from({ length: count }, (_, index) => ({ id: `term-${index}`, label: `Term ${index}` }));
  render(<Region element={region} views><input aria-label="Page" ref={field} defaultValue="417" />
    <ToolbarMenu label="Locator type" value="Page" filterLabel="Filter types" sections={[{ id: "terms", items }]} selectedKey={selected} placement="bottom start" returnFocusRef={field} onAction={onAction} /></Region>);
  return { field, onAction, region, trigger: screen.getByRole("button", { name: "Locator type Page" }) };
}

test("a menu opens on the side with room for its filter and four rows, whatever field focus returns to", async () => {
  let top = 60;
  const { trigger } = fitted(() => top, () => 480);
  fireEvent.click(trigger);
  await screen.findByRole("menu");
  // Its least: four whole rows of 28 px (the filter has no box in jsdom).
  expect(menuLeast(document.querySelector<HTMLElement>(".menu-popover")!)).toBe(112);
  await waitFor(() => expect(document.querySelector(".menu-popover")?.getAttribute("data-placement")).toBe("bottom"));
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
  // Low in the window it opens above: the return field below never limits it.
  top = 400;
  fireEvent.click(trigger);
  await screen.findByRole("menu");
  await waitFor(() => expect(document.querySelector(".menu-popover")?.getAttribute("data-placement")).toBe("top"));
});

test("a menu open while the window shrinks moves to the side that still has room", async () => {
  let height = 480;
  const { trigger } = fitted(() => 200, () => height);
  fireEvent.click(trigger);
  await screen.findByRole("menu");
  await waitFor(() => expect(document.querySelector(".menu-popover")?.getAttribute("data-placement")).toBe("bottom"));
  height = 260;
  await act(async () => { window.dispatchEvent(new Event("resize")); await new Promise(requestAnimationFrame); });
  await waitFor(() => expect(document.querySelector(".menu-popover")?.getAttribute("data-placement")).toBe("top"));
  expect(screen.getByRole("menu")).toBeTruthy();
});

test("with room for its least on neither side a menu opens as a view of the window, and Esc returns to the field with its draft", async () => {
  const { trigger, field, onAction } = fitted(() => 60, () => 150);
  act(() => field.current!.focus());
  fireEvent.click(trigger);
  const view = await screen.findByRole("dialog", { name: "Locator type" });
  expect(screen.queryByRole("menu")).toBeNull();
  const list = screen.getByRole("listbox", { name: "Locator type" });
  expect(list.querySelectorAll('[role="option"]')).toHaveLength(29);
  // Its filter takes the keyboard, as the menu's would; zero matches says so.
  expect(document.activeElement).toBe(view.querySelector("input"));
  fireEvent.change(document.activeElement!, { target: { value: "zzz" } });
  expect(screen.getByText("No matches")).toBeTruthy();
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(document.activeElement).toBe(field.current);
  expect(field.current!.value).toBe("417");
  expect(onAction).not.toHaveBeenCalled();
});

// Found in review: a window shrinking under an open, filtered menu turned it
// into a view that had forgotten the filter (20 matches became 29).
test("a menu that becomes a view keeps what was typed, its matches and its highlighted row", async () => {
  let height = 480;
  const { trigger } = fitted(() => 60, () => height);
  fireEvent.click(trigger);
  await screen.findByRole("menu");
  fireEvent.change(screen.getByRole("searchbox"), { target: { value: "2" } });
  await waitFor(() => expect(screen.getAllByRole("menuitemradio")).toHaveLength(11));
  fireEvent.keyDown(screen.getByRole("searchbox"), { key: "ArrowDown" });
  fireEvent.keyDown(screen.getByRole("searchbox"), { key: "ArrowDown" });
  const highlighted = await waitFor(() => { const row = document.querySelector<HTMLElement>("[data-focused][data-item]"); expect(row).toBeTruthy(); return row!.dataset.item; });
  height = 150;
  await act(async () => { window.dispatchEvent(new Event("resize")); await new Promise(requestAnimationFrame); });
  const view = await screen.findByRole("dialog", { name: "Locator type" });
  const field = view.querySelector("input")!;
  expect(field.value).toBe("2");
  expect(screen.getAllByRole("option")).toHaveLength(11);
  expect(document.getElementById(field.getAttribute("aria-activedescendant")!)?.id).toContain(highlighted);
});

// Found in review: a pointer that opens a cramped menu opens a view on
// pointerdown; that press's own mousedown then took the keyboard to the hidden
// trigger, leaving nothing focused, and the current row lay below the list.
test("a view opened by a press keeps the keyboard through that press and shows the current row", async () => {
  const { trigger } = fitted(() => 60, () => 150, 29, "term-20");
  const revealed = vi.mocked(Element.prototype.scrollIntoView);
  fireEvent.pointerDown(trigger, { button: 0, pointerType: "mouse" });
  const view = await screen.findByRole("dialog", { name: "Locator type" });
  const field = view.querySelector("input")!;
  // The press goes on: its mousedown moves no focus.
  expect(fireEvent.mouseDown(trigger, { button: 0 })).toBe(false);
  fireEvent.pointerUp(trigger, { button: 0, pointerType: "mouse" });
  expect(document.activeElement).toBe(field);
  const current = document.getElementById(field.getAttribute("aria-activedescendant")!)!;
  expect(current.textContent).toContain("Term 20");
  expect(revealed.mock.contexts).toContain(current);
  // A later press is the person's own.
  expect(fireEvent.mouseDown(document.body, { button: 0 })).toBe(true);
});

test.each(["blur", "pointercancel", "unmount", "new press"])("the opening press releases focus suppression on %s", async (end) => {
  const { trigger, field } = fitted(() => 60, () => 150);
  fireEvent.pointerDown(trigger, { button: 0, pointerType: "mouse", pointerId: 1, isPrimary: true });
  await screen.findByRole("dialog", { name: "Locator type" });
  if (end === "blur") fireEvent.blur(window);
  else if (end === "pointercancel") fireEvent.pointerCancel(document, { pointerId: 1 });
  else if (end === "unmount") fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  else fireEvent.pointerDown(field.current!, { button: 0, pointerType: "mouse", pointerId: 1, isPrimary: true });
  // No mouse event from the old press arrived before it ended.
  try { expect(fireEvent.mouseDown(trigger, { button: 0 })).toBe(true); }
  finally { fireEvent.pointerUp(document, { pointerId: 1 }); }
});

// Found in review: the footer's Library menu at 480 × 360 and 200 % text showed
// one row of four; a Menu, like a ToolbarMenu, opens as a view when cramped.
function fittedLibrary(height: () => number) {
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  Element.prototype.scrollIntoView = vi.fn();
  vi.spyOn(document.documentElement, "clientHeight", "get").mockImplementation(height);
  vi.spyOn(document.documentElement, "clientWidth", "get").mockReturnValue(480);
  const region = document.createElement("div");
  document.body.append(region);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this === region) return box(0, height(), 0, 480);
    if (this.matches('button[aria-label="Library"]')) return box(60, 24, 20, 100);
    const list = this.closest<HTMLElement>('[role="menu"]');
    if (!list) return new DOMRect();
    if (this === list) return box(0, list.clientHeight);
    const rows = [...list.querySelectorAll('[role^="menuitem"]')];
    // Row rectangles move with the scroll position, as they do in a browser.
    if (rows.includes(this)) return box(rows.indexOf(this) * 56 - list.scrollTop, 56);
    return new DOMRect();
  });
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this.matches(".menu-view [role=menu]") ? 112 : 0;
  });
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this.clientHeight;
  });
  const action = vi.fn();
  function Library() {
    const [open, setOpen] = useState(false);
    return <Region element={region} views><Menu label="Library" trigger="Library" isOpen={open} onOpenChange={setOpen}>
      <MenuSection id="actions" label="Library">
        {["Choose", "Find", "Open", "Reload"].map((name) => <MenuItem key={name} id={name} label={name} onAction={() => action(name)} />)}
      </MenuSection>
    </Menu></Region>;
  }
  render(<Library />);
  return { trigger: screen.getByRole("button", { name: "Library" }), action };
}

test("a Menu with room for its least on neither side opens as a view of the window, and Esc returns to its button", async () => {
  const { trigger } = fittedLibrary(() => 150);
  fireEvent.pointerDown(trigger, { button: 0, pointerType: "mouse" });
  // The popover is a dialog too, for the moment before it finds no room.
  const view = await waitFor(() => { const found = document.querySelector<HTMLElement>(".ui-view"); expect(found).toBeTruthy(); return found!; });
  expect(view.getAttribute("role")).toBe("dialog");
  expect(fireEvent.mouseDown(trigger, { button: 0 })).toBe(false);
  fireEvent.pointerUp(trigger, { button: 0, pointerType: "mouse" });
  const menu = screen.getByRole("menu", { name: "Library" });
  expect(view.contains(menu)).toBe(true);
  expect(document.querySelector(".menu-popover")).toBeNull();
  expect(screen.getAllByRole("menuitem")).toHaveLength(4);
  expect(view.contains(document.activeElement)).toBe(true);
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(document.activeElement).toBe(trigger);
});

test("a Menu that becomes a view keeps the focused row, scrolls it into view and acts on it", async () => {
  let height = 480;
  const { trigger, action } = fittedLibrary(() => height);
  fireEvent.keyDown(trigger, { key: "ArrowDown" });
  const menu = await screen.findByRole("menu", { name: "Library" });
  fireEvent.keyDown(document.activeElement!, { key: "End" });
  expect(document.activeElement).toBe(screen.getByRole("menuitem", { name: "Reload" }));
  height = 150;
  await act(async () => { fireEvent(window, new Event("resize")); await new Promise(requestAnimationFrame); });
  const view = await screen.findByRole("dialog", { name: "Library" });
  const list = screen.getByRole("menu", { name: "Library" });
  const reload = screen.getByRole("menuitem", { name: "Reload" });
  expect(list).not.toBe(menu);
  expect(view.contains(list)).toBe(true);
  expect(document.activeElement).toBe(reload);
  // React Aria checks the focused row's visibility again on the next frame.
  await act(async () => { await new Promise(requestAnimationFrame); });
  expect(list.scrollTop).toBe(112);
  fireEvent.keyDown(reload, { key: "Enter" });
  fireEvent.keyUp(reload, { key: "Enter" });
  expect(action).toHaveBeenCalledExactlyOnceWith("Reload");
});

test("a menu with no matches keeps its filter and empty state as its least", () => {
  const surface = document.createElement("div");
  surface.innerHTML = '<input><div role="menu"></div>';
  const list = surface.querySelector<HTMLElement>('[role="menu"]')!;
  Object.defineProperty(surface, "scrollHeight", { value: 52 });
  Object.defineProperty(list, "offsetHeight", { value: 8 });
  Object.defineProperty(list, "scrollHeight", { value: 8 });
  expect(menuLeast(surface)).toBe(52);
});
