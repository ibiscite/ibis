import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, renderHook, screen, waitFor, within } from "@testing-library/react";
import axe from "axe-core";
import { useRef, useState } from "react";
import { Menu, MenuDetail, MenuItem, MenuSection, MenuSeparator, useMenuBoundary, useMenuPlacement, useWholeRows } from "./Menu";
import { useMenuLifecycle } from "./useMenuLifecycle";

test.each(["frame", "cancel", "reopen", "unmount"])("a deferred menu focus request respects %s", async (action) => {
  render(<input aria-label="Retry target" />);
  const target = screen.getByRole("textbox");
  const hook = renderHook(({ open }) => useMenuLifecycle(open), { initialProps: { open: false } });
  act(() => hook.result.current.retryFocus(() => target.focus()));
  if (action === "cancel") act(() => hook.result.current.cancelFocusRetry());
  else if (action === "reopen") hook.rerender({ open: true });
  else if (action === "unmount") hook.unmount();
  await act(async () => { await new Promise(requestAnimationFrame); });
  expect(document.activeElement).toBe(action === "frame" ? target : document.body);
});

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

test("a menu trigger keeps its name when a narrow footer folds its visible label", () => {
  render(<Menu label="More" isOpen={false} onOpenChange={vi.fn()}
    trigger={<><span style={{ display: "none" }}>More</span><span aria-hidden="true">⌘K</span></>}>
    <MenuSection id="actions" label="Actions"><MenuItem id="open" label="Open" onAction={vi.fn()} /></MenuSection>
  </Menu>);
  expect(screen.getByRole("button", { name: "More" }).getAttribute("aria-expanded")).toBe("false");
});

test.each([undefined, "Open actions"])("an icon-only trigger uses its explicit name or the menu label (%s)", (triggerLabel) => {
  render(<Menu label="Actions" triggerLabel={triggerLabel} isOpen={false} onOpenChange={vi.fn()}
    trigger={<svg aria-hidden="true" />}>
    <MenuSection id="actions"><MenuItem id="open" label="Open" onAction={vi.fn()} /></MenuSection>
  </Menu>);
  expect(screen.getByRole("button", { name: triggerLabel ?? "Actions" }).getAttribute("aria-expanded")).toBe("false");
});

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

test("toggles reserve one leading column, and shortcuts stay out of names", async () => {
  const { trigger } = mount();
  fireEvent.click(trigger);
  const menu = await screen.findByRole("menu");
  const rows = [...menu.querySelectorAll("[role^=menuitem]")];
  expect(rows.map((row) => row.firstElementChild?.className)).toEqual(["menu-label", "menu-check", "menu-check"]);
  // The shared look reserves the same column for checks and icons.
  const menuCss = readFileSync(new FileURL("./Menu.css", import.meta.url), "utf8");
  expect(menuCss).toContain(':root .menu:has(.menu-check) :is(.menu-item:not(:has(> .menu-check)), .menu-header) { padding-inline-start: var(--menu-label-start); }');
  expect(screen.getByRole("menuitem", { name: "Open" }).textContent).toBe("OpenCtrl+O");
  // Menu shortcuts are trailing text, so global kbd cap styles cannot apply.
  const shortcut = screen.getByRole("menuitem", { name: "Open" }).querySelector(".menu-keys")!;
  expect(shortcut.tagName).toBe("SPAN");
  expect(shortcut.getAttribute("aria-hidden")).toBe("true");
  // The outer logical margin follows the menu; only the shortcut text is LTR.
  expect(shortcut.getAttribute("dir")).toBeNull();
  expect(shortcut.querySelector("bdi")?.getAttribute("dir")).toBe("ltr");
  const group = screen.getByRole("group", { name: "Sources" });
  expect(within(group).getAllByRole("menuitemcheckbox").map((row) => [row.textContent, row.getAttribute("aria-checked")]))
    .toEqual([["First", "true"], ["Second", "false"]]);
  expect(screen.getByRole("menuitemcheckbox", { name: "First" }).querySelector(".menu-check svg")).not.toBeNull();
  expect(screen.getByRole("menuitemcheckbox", { name: "Second" }).querySelector(".menu-check svg")).toBeNull();
  expect(within(menu).getAllByRole("separator")).toHaveLength(1);
});

test("icons and checks occupy one slot, including an icon on a toggle", async () => {
  render(<Menu label="Invented menu" trigger="Open menu" isOpen onOpenChange={vi.fn()}>
    <MenuSection id="mixed">
      <MenuItem id="icon" label="Invented library" icon={<span className="source-icon" />} onAction={vi.fn()} />
      <MenuItem id="plain" label="Invented action" onAction={vi.fn()} />
    </MenuSection>
    <MenuSection id="toggles">
      <MenuItem id="toggle" label="Invented toggle" checked icon={<span className="source-icon" />} onAction={vi.fn()} />
    </MenuSection>
  </Menu>);
  const icon = await screen.findByRole("menuitem", { name: "Invented library" });
  const toggle = screen.getByRole("menuitemcheckbox", { name: "Invented toggle" });
  for (const row of [icon, toggle]) {
    expect(row.querySelectorAll(":scope > .menu-check")).toHaveLength(1);
    expect(row.children[1]?.className).toBe("menu-label");
    expect(row.querySelector(":scope > .source-icon")).toBeNull();
  }
  expect(icon.querySelector(".menu-check > .source-icon")).not.toBeNull();
  expect(toggle.querySelector(".menu-check > svg")).not.toBeNull();
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
  // JSDOM has no CSS animation lifecycle: keep the real popover mounted until
  // its exit finishes, as the browser does for menu-exit in Menu.css.
  Object.defineProperty(popover, "getAnimations", { value: () => popover.hasAttribute("data-exiting")
    ? [{ playState: "running", finished }] : [] });
  return { popover, finish: async () => {
    await act(async () => { finishExit(); });
    await act(async () => { await new Promise(requestAnimationFrame); });
  } };
}

test.each(["pointer", "keyboard"])("%s action returns focus to Search through the menu's exit animation", async (activation) => {
  const { trigger, search, onAction } = mount();
  act(() => trigger.focus());
  fireEvent.click(trigger);
  const exit = holdExit(await screen.findByRole("menu"));
  const open = screen.getByRole("menuitem", { name: "Open" });
  act(() => open.focus());
  // The live modal popover makes the rest of the page inert until it closes.
  // Model its refusal of the action's first focus attempt (JSDOM has no inert).
  const focusSearch = search.focus.bind(search);
  const focus = vi.spyOn(search, "focus").mockImplementation(() => {
    if (!exit.popover.isConnected) focusSearch();
  });
  if (activation === "pointer") fireEvent.click(open);
  else {
    fireEvent.keyDown(open, { key: "Enter" });
    fireEvent.keyUp(open, { key: "Enter" });
  }
  expect(onAction).toHaveBeenCalledWith("open");
  expect(exit.popover.hasAttribute("data-exiting")).toBe(true);
  await exit.finish();
  focus.mockRestore();
  expect(screen.queryByRole("menu")).toBeNull();
  expect(document.activeElement).toBe(search);
});

test.each(["Escape", "Tab"])("%s restores its destination after an animated exit", async (key) => {
  const { trigger, search } = mount();
  act(() => trigger.focus());
  fireEvent.click(trigger);
  const menu = await screen.findByRole("menu");
  const exit = holdExit(menu);
  act(() => menu.focus());
  fireEvent.keyDown(menu, { key });
  await exit.finish();
  expect(document.activeElement).toBe(key === "Escape" ? trigger : search);
});

test.each([false, true])("Tab (shift=%s) releases the modal before immediate typing", async (shiftKey) => {
  const { trigger, search } = mount();
  act(() => trigger.focus());
  fireEvent.keyDown(trigger, { key: "ArrowDown" });
  const menu = await screen.findByRole("menu");
  const exit = holdExit(menu);
  const focusSearch = search.focus.bind(search);
  const focus = vi.spyOn(search, "focus").mockImplementation(() => {
    if (!exit.popover.isConnected) focusSearch();
  });
  fireEvent.keyDown(document.activeElement!, { key: "Tab", shiftKey });
  // No animation completion or frame is allowed before the next input.
  expect(exit.popover.isConnected).toBe(false);
  expect(document.activeElement).toBe(search);
  fireEvent.input(document.activeElement!, { target: { value: "immediate" } });
  expect(search.value).toBe("immediate");
  focus.mockRestore();
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

test("a shortcut can reopen immediately after Tab and action exits still animate", async () => {
  const { trigger, search } = mount();
  fireEvent.click(trigger);
  fireEvent.keyDown(await screen.findByRole("menu"), { key: "Tab" });
  expect(document.activeElement).toBe(search);
  fireEvent.keyDown(search, { key: "k", ctrlKey: true });
  const reopened = await screen.findByRole("menu");
  const exit = holdExit(reopened);
  fireEvent.click(screen.getByRole("menuitem", { name: "Open" }));
  expect(exit.popover.hasAttribute("data-exiting")).toBe(true);
  await exit.finish();
  expect(document.activeElement).toBe(search);
});

test.each(["focus", "pointer", "keyboard"])("later %s intent supersedes the action's focus destination", async (intent) => {
  const { trigger, search } = mount();
  fireEvent.click(trigger);
  const exit = holdExit(await screen.findByRole("menu"));
  fireEvent.click(screen.getByRole("menuitem", { name: "Open" }));
  // A dialog or pointer-chosen control can take focus. A later key can also
  // leave focus on the trigger without reopening the menu.
  const target = intent === "keyboard" ? trigger : document.createElement("button");
  if (intent !== "keyboard") document.body.append(target);
  if (intent === "pointer") {
    fireEvent.pointerDown(target, { button: 0, pointerType: "mouse", pointerId: 1 });
    fireEvent.pointerUp(target, { button: 0, pointerType: "mouse", pointerId: 1 });
    fireEvent.click(target);
  }
  else if (intent === "keyboard") fireEvent.keyDown(target, { key: "Shift" });
  act(() => target.focus());
  await exit.finish();
  expect(document.activeElement).toBe(target);
  expect(document.activeElement).not.toBe(search);
  if (intent !== "keyboard") target.remove();
});

test.each(["action", "Escape"])("rapid pointer reopening after %s keeps the menu usable through its exit", async (close) => {
  const { trigger } = mount();
  fireEvent.click(trigger);
  const menu = await screen.findByRole("menu");
  const item = screen.getByRole("menuitem", { name: "Open" });
  act(() => item.focus());
  const popover = menu.closest(".menu-popover")!;
  let finishExit!: () => void;
  const finished = new Promise<void>((resolve) => { finishExit = resolve; });
  Object.defineProperty(popover, "getAnimations", { value: () => popover.hasAttribute("data-exiting")
    ? [{ playState: "running", finished }] : [] });
  if (close === "action") fireEvent.click(item);
  else fireEvent.keyDown(item, { key: "Escape" });
  expect(popover.hasAttribute("data-exiting")).toBe(true);
  // The browser blurs the trigger when reopening makes the page inert again.
  // JSDOM has no inert focus behavior; model that loss before the open commit.
  act(() => (document.activeElement as HTMLElement).blur());
  const focus = vi.spyOn(trigger, "focus").mockImplementation(() => {});
  try {
    fireEvent.pointerDown(trigger, { button: 0, pointerType: "mouse", pointerId: 1 });
    fireEvent.pointerUp(trigger, { button: 0, pointerType: "mouse", pointerId: 1 });
    fireEvent.click(trigger);
    await act(async () => { await new Promise(requestAnimationFrame); });
    const reopened = screen.getByRole("menu");
    expect(reopened.closest(".menu-popover")).toBe(popover);
    expect(document.activeElement).toBe(reopened);
    expect(reopened.querySelector("[data-focused]")).toBeNull();
    fireEvent.keyDown(document.activeElement!, { key: "ArrowDown" });
    expect(document.activeElement).toBe(within(reopened).getByRole("menuitem", { name: "Open" }));
  } finally {
    focus.mockRestore();
    await act(async () => { finishExit(); });
    await act(async () => { await new Promise(requestAnimationFrame); });
  }
});

const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
const menuCss = readFileSync(new FileURL("./Menu.css", import.meta.url), "utf8");
const styles = readFileSync(new FileURL("../styles.css", import.meta.url), "utf8");
test("menus highlight with a fill, never a ring, and the picker sheet has no menu rules of its own", () => {
  expect(menuCss).toMatch(/\.menu-item\[data-focused\] \{ background: var\(--menu-highlight\); color: var\(--menu-highlight-text\); \}/);
  expect(menuCss).toMatch(/\.menu-item \{[^}]*outline: none/);
  expect(menuCss).toMatch(/\.menu-popover \[role="menu"\] \{[^}]*outline: none/);
  // The global :focus-visible ring and its halo would otherwise draw lines on the list.
  expect(menuCss).toMatch(/\.menu-popover \[role="menu"\]:focus-visible \{ outline: none; box-shadow: none; \}/);
  expect(styles).not.toMatch(/role="menu|menuitem/);
});

test("an open menu bounds placement below the return field and removes the boundary on close", async () => {
  const { trigger, search } = mount();
  vi.spyOn(search, "getBoundingClientRect").mockReturnValue(new DOMRect(8, 8, 240, 40));
  fireEvent.click(trigger);
  await screen.findByRole("menu");
  let boundary = document.querySelector<HTMLElement>("[data-menu-boundary]");
  expect(boundary?.style.position).toBe("fixed");
  expect(boundary?.style.top).toBe("48px");
  expect(boundary?.getAttribute("aria-hidden")).toBe("true");
  vi.mocked(search.getBoundingClientRect).mockReturnValue(new DOMRect(8, 8, 240, 56));
  fireEvent(window, new Event("resize"));
  await act(async () => { await new Promise(requestAnimationFrame); });
  expect(boundary?.isConnected).toBe(false);
  boundary = document.querySelector<HTMLElement>("[data-menu-boundary]");
  expect(boundary?.style.top).toBe("64px");
  fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
  await waitFor(() => expect(boundary?.isConnected).toBe(false));
});

test("placement preserves a visible header anchor while bounding an off-window footer", () => {
  const search = document.createElement("input");
  vi.spyOn(search, "getBoundingClientRect").mockReturnValue(new DOMRect(8, 8, 240, 36));
  const ref = { current: search };
  const hook = renderHook(() => useMenuBoundary(true, ref));
  vi.spyOn(hook.result.current.boundaryElement!, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 44, 720, 596));
  const button = document.createElement("button");
  const rect = vi.spyOn(button, "getBoundingClientRect").mockReturnValue(new DOMRect(590, 12, 82, 28));
  expect(hook.result.current.getTargetRect(button).top).toBe(12);
  rect.mockReturnValue(new DOMRect(590, 680, 82, 24));
  const bounded = hook.result.current.getTargetRect(button);
  expect(bounded.top).toBeGreaterThanOrEqual(44);
  expect(bounded.top).toBeLessThan(640);
  hook.unmount();
});

test.each(["ltr", "rtl"])("menu alignment follows rendered %s on opening and keeps its closing edge", (direction) => {
  const button = document.createElement("button");
  button.dir = direction === "rtl" ? "ltr" : "rtl";
  button.style.direction = direction;
  document.body.append(button);
  const trigger = { current: button };
  const hook = renderHook(({ open, placement }: { open: boolean; placement: "top end" | "bottom end" }) => useMenuPlacement(open, trigger, placement),
    { initialProps: { open: true, placement: "bottom end" } });
  expect(hook.result.current).toBe(direction === "rtl" ? "bottom left" : "bottom right");
  button.style.direction = direction === "rtl" ? "ltr" : "rtl";
  hook.rerender({ open: false, placement: "top end" });
  expect(hook.result.current).toBe(direction === "rtl" ? "top left" : "top right");
  hook.rerender({ open: true, placement: "top end" });
  expect(hook.result.current).toBe(direction === "rtl" ? "top right" : "top left");
  hook.unmount();
  button.remove();
});

// #1031: faults measured in a browser render of the format and More menus.
test("a menu opened above the footer ends above it; a footer menu keeps the whole window", () => {
  const footer = document.createElement("footer");
  footer.dataset.menuFloor = "";
  const more = document.createElement("button");
  footer.append(more);
  const header = document.createElement("button");
  document.body.append(header, footer);
  vi.spyOn(footer, "getBoundingClientRect").mockReturnValue(new DOMRect(0, window.innerHeight - 40, 720, 40));
  const [headerRef, moreRef] = [{ current: header }, { current: more }];
  const above = renderHook(() => useMenuBoundary(true, undefined, headerRef));
  expect(above.result.current.boundaryElement?.style.bottom).toBe("40px");
  above.unmount();
  const inside = renderHook(() => useMenuBoundary(true, undefined, moreRef));
  // Untouched: the boundary keeps its "inset: 0" bottom at the window edge.
  expect(inside.result.current.boundaryElement?.style.bottom).not.toBe("40px");
  inside.unmount();
  footer.remove();
  header.remove();
});

test("a scrolling menu shortens to its last whole row and refits when the popover resizes", async () => {
  const frame = document.createElement("div");
  const list = document.createElement("div");
  frame.append(list);
  document.body.append(frame);
  const rows = [0, 1, 2, 3].map((i) => {
    const row = document.createElement("div");
    row.setAttribute("role", "menuitem");
    vi.spyOn(row, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 100 + i * 28, 200, 28));
    list.append(row);
    return row;
  });
  vi.spyOn(list, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 100, 200, 70));
  let height = 70;
  Object.defineProperty(list, "clientHeight", { get: () => height });
  Object.defineProperty(list, "scrollHeight", { get: () => rows.length * 28 });
  const listRef = { current: list };
  const hook = renderHook(() => useWholeRows(listRef));
  act(() => hook.result.current(frame));
  // 70 px would cut the third 28 px row in half; two whole rows remain.
  expect(list.style.maxHeight).toBe("56px");
  // More rows wait below; scrolled to the end, only those above remain.
  expect([list.hasAttribute("data-more-above"), list.hasAttribute("data-more-below")]).toEqual([false, true]);
  list.scrollTop = 56;
  fireEvent.scroll(list);
  expect([list.hasAttribute("data-more-above"), list.hasAttribute("data-more-below")]).toEqual([true, false]);
  list.scrollTop = 0;
  height = 100;
  frame.style.maxHeight = "200px";
  await act(async () => { await Promise.resolve(); });
  expect(list.style.maxHeight).toBe("84px");
  height = 200;
  frame.style.maxHeight = "300px";
  await act(async () => { await Promise.resolve(); });
  expect(list.style.maxHeight).toBe("");
  expect(list.hasAttribute("data-more-below")).toBe(false);
  act(() => hook.result.current(null));
  frame.remove();
});

test("any pointer movement over an item highlights it, also after arrow keys", async () => {
  const { search } = mount();
  act(() => search.focus());
  fireEvent.keyDown(search, { key: "k", ctrlKey: true });
  const menu = await screen.findByRole("menu");
  const open = screen.getByRole("menuitem", { name: "Open" });
  const second = screen.getByRole("menuitemcheckbox", { name: "Second" });
  expect(open.hasAttribute("data-focused")).toBe(true);
  // The pointer rests on Second while the keyboard moves the highlight.
  fireEvent.pointerMove(second, { pointerType: "mouse", clientX: 1 });
  expect(second.hasAttribute("data-focused")).toBe(true);
  fireEvent.keyDown(document.activeElement!, { key: "ArrowUp" });
  await waitFor(() => expect(second.hasAttribute("data-focused")).toBe(false));
  // The synthetic move a resting pointer gets when rows scroll under it does not count.
  fireEvent.pointerMove(second, { pointerType: "mouse", clientX: 1 });
  expect(second.hasAttribute("data-focused")).toBe(false);
  fireEvent.pointerMove(second, { pointerType: "mouse", clientX: 3 });
  expect(second.hasAttribute("data-focused")).toBe(true);
  expect(document.activeElement).toBe(second);
  // Touch never moves the highlight.
  fireEvent.pointerMove(open, { pointerType: "touch", clientX: 5 });
  expect(open.hasAttribute("data-focused")).toBe(false);
  expect(menu.isConnected).toBe(true);
});

test("rows keep equal insets, fit intrinsic labels to the viewport and wrap at its limit", () => {
  const popover = menuCss.slice(menuCss.indexOf(".menu-popover {\n  box-sizing"));
  expect(popover.slice(0, popover.indexOf("}"))).toMatch(/padding-block: var\(--menu-padding\);/);
  expect(popover.slice(0, popover.indexOf("}"))).not.toMatch(/[^-]padding: /);
  expect(menuCss).toMatch(/\.menu-popover \[role="menu"\] \{[^}]*padding-inline: var\(--menu-padding\);/);
  expect(menuCss).toMatch(/\.menu-popover > :not\(\[role="menu"\]\) \{ margin-inline: var\(--menu-padding\); \}/);
  expect(menuCss).toMatch(/\.menu-popover \{[^}]*width: max-content;/);
  expect(menuCss).toMatch(/\.menu-label \{[^}]*white-space: normal;/);
  expect(menuCss).not.toMatch(/\.menu-label \{[^}]*text-overflow: ellipsis/);
  expect(menuCss).toMatch(/--menu-label-start: calc\(var\(--menu-item-pad\) \+ var\(--menu-check-width\) \+ var\(--menu-item-gap\)\);/);
  const toolbar = readFileSync(new FileURL("./ToolbarMenu.css", import.meta.url), "utf8");
  // The filter text and section headings start where the labels do.
  expect(toolbar.match(/padding-inline: var\(--menu-label-start\) var\(--menu-item-pad\)/g)).toHaveLength(2);
  expect(toolbar).toMatch(/\.toolbar-menu-popover \{[^}]*min-width: min\(300px, calc\(100vw - 16px\)\);/);
  expect(toolbar).toMatch(/\.toolbar-menu-sample \{[^}]*min-width: 0;[^}]*white-space: normal;/);
  expect(menuCss).toContain("--menu-check-width: var(--icon-small)");
  expect(`${menuCss}${toolbar}`).not.toMatch(/\b(14|34)px\b/);
});

test("the shortcuts table breaks actions only between words and keeps keys on one line at the end", () => {
  expect(styles).not.toMatch(/\.keys-scroll[^{]*\{[^}]*overflow-wrap: anywhere/);
  expect(styles).toMatch(/\.keys-scroll th, \.keys-scroll td \{[^}]*overflow-wrap: break-word;/);
  expect(styles).toMatch(/\.keys-scroll td \{[^}]*text-align: end; white-space: nowrap; \}/);
});


test("whole-row fitting preserves variable-height wrapped action rows", () => {
  const frame = document.createElement("div"), list = document.createElement("div");
  frame.append(list); document.body.append(frame);
  const geometry = [[0, 28], [28, 56.4], [84.4, 56]];
  for (const [top, height] of geometry) {
    const row = document.createElement("div"); row.setAttribute("role", "menuitem"); list.append(row);
    vi.spyOn(row, "getBoundingClientRect").mockReturnValue(new DOMRect(0, top, 200, height));
  }
  Object.defineProperty(list, "clientHeight", { value: 90 });
  Object.defineProperty(list, "scrollHeight", { value: 140 });
  const hook = renderHook(() => useWholeRows({ current: list }));
  act(() => hook.result.current(frame));
  expect(list.style.maxHeight).toBe("85px");
  act(() => hook.result.current(null)); frame.remove();
});


test("focus, row resize and reopening update safe fades without an old opening or manual scroll taking over", async () => {
  const callbacks: (() => void)[] = [];
  vi.stubGlobal("ResizeObserver", class { constructor(callback: () => void) { callbacks.push(callback); } observe() {} disconnect() {} });
  const frame = document.createElement("div"), list = document.createElement("div"), row = document.createElement("div");
  row.setAttribute("role", "menuitem"); row.dataset.focused = "true";
  list.append(row); frame.append(list); document.body.append(frame);
  Object.defineProperty(list, "clientHeight", { value: 100 });
  Object.defineProperty(list, "scrollHeight", { value: 300 });
  vi.spyOn(list, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 0, 200, 100));
  let top = 60;
  vi.spyOn(row, "getBoundingClientRect").mockImplementation(() => new DOMRect(0, top, 200, 40));
  const ref = { current: list }, hook = renderHook(() => useWholeRows(ref));
  act(() => hook.result.current(frame));
  expect(list.style.getPropertyValue("--scroll-fade-below-space")).toBe("0px");
  top = 0;
  await act(async () => { row.setAttribute("data-focused", ""); });
  expect(list.style.getPropertyValue("--scroll-fade-above-space")).toBe("0px");
  list.scrollTop = 70; fireEvent.scroll(list);
  expect(list.scrollTop).toBe(70);
  const oldResize = callbacks[0]!;
  const next = document.createElement("div"); document.body.append(next);
  act(() => { hook.result.current(null); next.append(list); hook.result.current(next); });
  top = 30;
  act(() => oldResize());
  expect(list.style.getPropertyValue("--scroll-fade-above-space")).toBe("0px");
  act(() => callbacks.at(-1)!());
  expect(list.style.getPropertyValue("--scroll-fade-above-space")).toBe("30px");
  expect(list.scrollTop).toBe(70);
  act(() => hook.result.current(null)); frame.remove(); next.remove();
});


test("a short viewport does not sacrifice the output list to footer clearance", () => {
  vi.stubGlobal("innerHeight", 320);
  const footer = document.createElement("footer"), trigger = document.createElement("button");
  footer.dataset.menuFloor = ""; document.body.append(trigger, footer);
  vi.spyOn(footer, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 132, 360, 188));
  const triggerRef = { current: trigger };
  const hook = renderHook(() => useMenuBoundary(true, undefined, triggerRef));
  expect(hook.result.current.boundaryElement?.style.bottom).toBe("0px");
  hook.unmount(); footer.remove(); trigger.remove();
});

test.each([
  ["uniform", Array.from({ length: 10 }, () => 28)],
  ["wrapped", [28, 56, 28, 56, 28, 56, 28]],
] as const)("refitting a scrolled %s menu keeps its last visible row whole and preserves the scroll position", async (_name, heights) => {
  const frame = document.createElement("div"), list = document.createElement("div");
  frame.append(list); document.body.append(frame);
  let availableHeight = 140, rowTop = 0;
  Object.defineProperty(list, "clientHeight", { get: () => Math.min(availableHeight, parseFloat(list.style.maxHeight) || Infinity) });
  Object.defineProperty(list, "scrollHeight", { value: heights.reduce((total, height) => total + height, 0) });
  vi.spyOn(list, "getBoundingClientRect").mockImplementation(() => new DOMRect(0, 100, 300, list.clientHeight));
  for (const height of heights) {
    const top = rowTop;
    const row = document.createElement("div"); row.setAttribute("role", "menuitem"); list.append(row);
    vi.spyOn(row, "getBoundingClientRect").mockImplementation(() => new DOMRect(0, 100 + top - list.scrollTop, 300, height));
    rowTop += height;
  }
  const hook = renderHook(() => useWholeRows({ current: list }));
  act(() => hook.result.current(frame));
  list.scrollTop = 70;
  fireEvent.scroll(list);
  availableHeight = 100;
  await act(async () => { frame.style.maxHeight = "114px"; });
  // Row rectangles already include scrollTop: the last whole visible row ends
  // 98px below this viewport's top. An 84px cap cuts through that row.
  expect(list.style.maxHeight).toBe("98px");
  const viewportBottom = list.getBoundingClientRect().bottom;
  expect([...list.children].some((row) => row.getBoundingClientRect().bottom === viewportBottom)).toBe(true);
  expect(list.scrollTop).toBe(70);
  availableHeight = 128;
  await act(async () => { frame.style.maxHeight = "142px"; });
  expect(list.style.maxHeight).toBe("126px");
  expect(list.scrollTop).toBe(70);
  act(() => hook.result.current(null)); frame.remove();
});

test("wrapped menu labels retain vertical padding and scalable minimum heights", () => {
  expect(menuCss).toMatch(/\.menu-item \{[^}]*padding-block: var\(--space-1\)/);
  expect(menuCss).toMatch(/--menu-item-height: 1\.5rem/);
  expect(menuCss).toMatch(/font: var\(--type-meta-size\) \/ var\(--type-meta-line\)/);
});
