import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import axe from "axe-core";
import { useRef, useState } from "react";
import { Menu, MenuItem, MenuSection, MenuSeparator } from "./Menu";

afterEach(() => { cleanup(); vi.unstubAllGlobals(); delete document.documentElement.dataset.platform; });

function Harness({ onAction, toggle }: { onAction: (id: string) => void; toggle: (id: string) => void }) {
  const [open, setOpen] = useState(false);
  const [on, setOn] = useState(new Set(["first"]));
  const search = useRef<HTMLInputElement>(null);
  const flip = (id: string) => { toggle(id); setOn((now) => { const next = new Set(now); if (next.has(id)) next.delete(id); else next.add(id); return next; }); };
  return <main onKeyDown={(event) => { if (event.key === "k" && event.ctrlKey) setOpen((now) => !now); }}>
    <input ref={search} aria-label="Search" />
    <Menu label="Actions" trigger="Actions" isOpen={open} onOpenChange={setOpen} tabFocusRef={search}>
      <MenuSection id="plain">
        <MenuItem id="open" label="Open" keys="Ctrl+O" onAction={() => onAction("open")} />
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
  expect(document.activeElement).toBe(trigger);

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
  await screen.findByRole("menu");
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
  expect(document.activeElement).toBe(search);
  expect(onAction).not.toHaveBeenCalled();
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
