import { cleanup, render } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { KeyCaps, KeyHint } from "./KeyCaps";
import { PickerFooter } from "../features/picker/PickerFooter";
import { MoreMenu } from "../features/picker/MoreMenu";

const caps = (container: HTMLElement) => [...container.querySelectorAll("kbd")].map((cap) => cap.textContent);

afterEach(() => { cleanup(); vi.restoreAllMocks(); delete document.documentElement.dataset.platform; });

test("implicit key caps follow the rendered look when it differs from the host and changes", () => {
  vi.spyOn(navigator, "platform", "get").mockReturnValue("MacIntel");
  document.documentElement.dataset.platform = "windows";
  const { container, rerender } = render(<KeyCaps accelerator="CmdOrCtrl+K" />);
  expect(caps(container)).toEqual(["Ctrl", "K"]);
  document.documentElement.dataset.platform = "macos";
  rerender(<KeyCaps accelerator="CmdOrCtrl+K" />);
  expect(caps(container)).toEqual(["⌘K"]);
  document.documentElement.dataset.platform = "linux";
  rerender(<KeyCaps accelerator="CmdOrCtrl+K" />);
  expect(caps(container)).toEqual(["Ctrl", "K"]);
  expect(container.querySelectorAll(".key-plus")).toHaveLength(1);
});

test("footer and More pair visible keys with their accessible shortcut after a look change", () => {
  vi.spyOn(navigator, "platform", "get").mockReturnValue("MacIntel");
  const footer = () => <PickerFooter library={null} groupName={() => ""} hints={{ enter: "Copy", shiftEnter: "Add", escape: "Hide" }} altHeld={false}
    actions={{ copy: vi.fn(), add: vi.fn(), escape: vi.fn(), keys: vi.fn() }}
    more={<MoreMenu isOpen={false} onOpenChange={vi.fn()} buttonRef={{ current: null }} searchRef={{ current: null }} keys="CmdOrCtrl+K">{null}</MoreMenu>} />;
  document.documentElement.dataset.platform = "windows";
  const { container, rerender } = render(footer());
  const check = (shortcut: string, more: string, keyCaps: string[], moreCaps: string[]) => {
    const shortcuts = container.querySelector<HTMLButtonElement>('.shortcuts-hint')!;
    const trigger = container.querySelector<HTMLButtonElement>('.more > button')!;
    expect(shortcuts.getAttribute("aria-keyshortcuts")).toBe(shortcut);
    expect(caps(shortcuts)).toEqual(keyCaps);
    expect(trigger.getAttribute("aria-keyshortcuts")).toBe(more);
    expect(caps(trigger)).toEqual(moreCaps);
  };
  check("Control+/", "Control+K", ["Ctrl", "/"], ["Ctrl", "K"]);
  document.documentElement.dataset.platform = "macos";
  rerender(footer());
  check("Meta+/", "Meta+K", ["⌘/"], ["⌘K"]);
  document.documentElement.dataset.platform = "linux";
  rerender(footer());
  check("Control+/", "Control+K", ["Ctrl", "/"], ["Ctrl", "K"]);
});

test("macOS joins the glyphs in one cap, as its menus write them", () => {
  const { container } = render(<KeyCaps accelerator="CmdOrCtrl+K" platform="macos" />);
  expect(caps(container)).toEqual(["⌘K"]);
  expect(container.querySelector(".key-plus")).toBeNull();
});

test("Windows gives each key its own cap, with nothing between", () => {
  const { container } = render(<KeyCaps accelerator="CmdOrCtrl+K" platform="windows" />);
  expect(caps(container)).toEqual(["Ctrl", "K"]);
  expect(container.querySelector(".key-plus")).toBeNull();
});

test("GNOME gives each key its own cap with a plus between, as GTK's shortcut labels do", () => {
  const { container } = render(<KeyCaps accelerator="Shift+Enter" platform="linux" />);
  expect(caps(container)).toEqual(["Shift", "Enter"]);
  expect([...container.querySelectorAll(".key-plus")].map((plus) => plus.textContent)).toEqual(["+"]);
});

test("a lone plus or slash key stays one cap", () => {
  const { container } = render(<KeyCaps accelerator="CmdOrCtrl+/" platform="windows" />);
  expect(caps(container)).toEqual(["Ctrl", "/"]);
});

test("a hint names the action first and its keys after", () => {
  const { container } = render(<KeyHint label="Copy as Pandoc" keys="Enter" platform="windows" primary />);
  const hint = container.firstElementChild!;
  expect(hint.firstElementChild?.className).toBe("key-hint-label");
  expect(hint.lastElementChild?.className).toBe("key-caps");
  expect(hint.textContent).toBe("Copy as Pandoc Enter");
  expect(hint.getAttribute("data-primary")).toBe("true");
});
