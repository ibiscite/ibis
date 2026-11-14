import { cleanup, render } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { KeyCaps, KeyHint } from "./KeyCaps";
import { PickerFooter } from "../features/picker/PickerFooter";
import { PickerMenu } from "../features/picker/PickerMenu";
import { KeysDialog } from "./KeysSheet";
import { setPlatform } from "../nativeAppearance";

const caps = (container: HTMLElement) => [...container.querySelectorAll("kbd")].map((cap) => cap.textContent);

afterEach(() => { cleanup(); vi.restoreAllMocks(); delete document.documentElement.dataset.platform; delete document.documentElement.dataset.system; });

test("implicit key caps retain actual system modifiers under the shared macOS look", () => {
  vi.spyOn(navigator, "platform", "get").mockReturnValue("MacIntel");
  setPlatform("Win32");
  const { container, rerender } = render(<KeyCaps accelerator="CmdOrCtrl+K" />);
  expect(caps(container)).toEqual(["Ctrl+K"]);
  setPlatform("MacIntel");
  rerender(<KeyCaps accelerator="CmdOrCtrl+K" />);
  expect(caps(container)).toEqual(["⌘K"]);
  setPlatform("Linux x86_64");
  rerender(<KeyCaps accelerator="CmdOrCtrl+K" />);
  expect(caps(container)).toEqual(["Ctrl+K"]);
});

test("footer and More pair actual system keys with their accessible shortcuts", () => {
  vi.spyOn(navigator, "platform", "get").mockReturnValue("MacIntel");
  const footer = () => <PickerFooter library={null} groupName={() => ""} hints={{ enter: "Copy", shiftEnter: "Add", escape: "Hide" }} altHeld={false}
    actions={{ copy: vi.fn(), add: vi.fn(), escape: vi.fn() }}
    more={<PickerMenu isOpen={false} onOpenChange={vi.fn()} buttonRef={{ current: null }} searchRef={{ current: null }} libraryDetail={null} pinned={false} keepOrder={null}
      actions={{ pin: vi.fn(), keys: vi.fn(), settings: null, format: null, chooseSource: vi.fn(), revealSource: null, quit: vi.fn() }} />} />;
  setPlatform("Win32");
  const { container, rerender } = render(footer());
  const check = (more: string, moreCaps: string[], copyCap: string) => {
    const copy = container.querySelector<HTMLButtonElement>('.footer-action')!;
    const trigger = container.querySelector<HTMLButtonElement>('.more > button')!;
    expect(copy.getAttribute("aria-keyshortcuts")).toBe("Enter");
    expect(container.querySelector(".shortcuts-hint")).toBeNull();
    expect(caps(copy)).toEqual([copyCap]);
    expect(trigger.getAttribute("aria-keyshortcuts")).toBe(more);
    expect(caps(trigger)).toEqual(moreCaps);
  };
  check("Control+K", ["Ctrl+K"], "Enter");
  setPlatform("MacIntel");
  rerender(footer());
  check("Meta+K", ["⌘K"], "↵");
  setPlatform("Linux x86_64");
  rerender(footer());
  check("Control+K", ["Ctrl+K"], "Enter");
});

test("macOS joins the glyphs in one cap, as its menus write them", () => {
  const { container } = render(<KeyCaps accelerator="CmdOrCtrl+K" platform="macos" />);
  expect(caps(container)).toEqual(["⌘K"]);
});

test("Windows and GNOME write the whole shortcut in one cap, as their menus do", () => {
  const { container, rerender } = render(<KeyCaps accelerator="CmdOrCtrl+K" platform="windows" />);
  expect(caps(container)).toEqual(["Ctrl+K"]);
  rerender(<KeyCaps accelerator="Shift+Enter" platform="linux" />);
  expect(caps(container)).toEqual(["Shift+Enter"]);
});

test("a lone plus or slash key stays inside the one cap", () => {
  const { container } = render(<KeyCaps accelerator="CmdOrCtrl+/" platform="windows" />);
  expect(caps(container)).toEqual(["Ctrl+/"]);
});

test("a hint names the action first and its keys after", () => {
  const { container } = render(<KeyHint label="Copy as Pandoc" keys="Enter" platform="windows" primary />);
  const hint = container.firstElementChild!;
  expect(hint.firstElementChild?.className).toBe("key-hint-label");
  expect(hint.lastElementChild?.className).toBe("key-caps");
  expect(hint.textContent).toBe("Copy as Pandoc Enter");
  expect(hint.getAttribute("data-primary")).toBe("true");
});

test("the shortcuts list pairs the native chord with its accessible shortcut", () => {
  render(<KeysDialog bindings={[
    { region: "picker", action: "pin", keys: ["CmdOrCtrl+Shift+P"] },
  ]} isMac onClose={vi.fn()} />);
  const cap = document.querySelector("kbd")!;
  expect(cap.textContent).toBe("⇧⌘P");
  expect(cap.closest("[aria-keyshortcuts]")?.getAttribute("aria-keyshortcuts")).toBe("Meta+Shift+P");
});
