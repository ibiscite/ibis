import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import axe from "axe-core";
import { useRef, useState } from "react";
import { Menu, MenuItem, MenuSection, MenuSeparator } from "./Menu";

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
      <MenuSection id="plain">
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
  return { onAction, toggle, trigger: screen.getByRole("button", { name: "Actions" }), search: screen.getByRole("textbox") };
}

test.each(["macos", "windows", "linux"])("%s: a pointer opens with nothing highlighted, a shortcut highlights the first item, axe passes", async (platform) => {
  document.documentElement.dataset.platform = platform;
  const { trigger, search } = mount();
  fireEvent.pointerDown(trigger, { button: 0, pointerType: "mouse" });
  const menu = await screen.findByRole("menu", { name: "Actions" });
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

test("every row has a leading check column; toggles are checkbox items and the shortcut is not in the name", async () => {
  const { trigger } = mount();
  fireEvent.click(trigger);
  const menu = await screen.findByRole("menu");
  const rows = [...menu.querySelectorAll("[role^=menuitem]")];
  expect(rows.map((row) => row.firstElementChild?.className)).toEqual(["menu-check", "menu-check", "menu-check"]);
  expect(screen.getByRole("menuitem", { name: "Open" }).textContent).toBe("OpenCtrl+O");
  const group = screen.getByRole("group", { name: "Sources" });
  expect(within(group).getAllByRole("menuitemcheckbox").map((row) => [row.textContent, row.getAttribute("aria-checked")]))
    .toEqual([["First", "true"], ["Second", "false"]]);
  expect(screen.getByRole("menuitemcheckbox", { name: "First" }).querySelector(".menu-check svg")).not.toBeNull();
  expect(screen.getByRole("menuitemcheckbox", { name: "Second" }).querySelector(".menu-check svg")).toBeNull();
  expect(within(menu).getAllByRole("separator")).toHaveLength(1);
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
  expect(styles).not.toMatch(/role="menu|menuitem/);
});
