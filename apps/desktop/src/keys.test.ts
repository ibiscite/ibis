import { expect, test } from "vitest";
import { KEY_TABLE, keyLabel, matchesAccelerator, shortcutLabel } from "./keys";
import pickerCommands from "./testing/picker-command-shortcuts.json";

test("Windows and Linux write modifiers Ctrl, Alt, Shift, as their menus do; macOS ⌃⌥⇧⌘", () => {
  expect(keyLabel("Alt+CmdOrCtrl+O", false)).toBe("Ctrl+Alt+O");
  expect(keyLabel("Shift+Alt+CmdOrCtrl+O", false)).toBe("Ctrl+Alt+Shift+O");
  expect(keyLabel("Alt+CmdOrCtrl+O", true)).toBe("⌥⌘O");
  expect(shortcutLabel("Shift+Control+Y", false)).toBe("Ctrl+Shift+Y");
  expect(shortcutLabel("Shift+Alt+Super+Y", false)).toBe("Alt+Shift+Super+Y");
});

test("native shortcut refusals cover every modified picker command", () => {
  // A click is no shortcut, so the recorder never sees one.
  const commands = KEY_TABLE.flatMap((binding) => binding.keys.filter((keys) => keys.includes("CmdOrCtrl") && !keys.endsWith("+Click")));
  // isKeysShortcut also accepts the shifted punctuation used to type ?.
  expect(new Set(pickerCommands)).toEqual(new Set([...commands, "CmdOrCtrl+Shift+/"]));
});

const press = (key: string, more: Partial<{ code: string; ctrlKey: boolean; metaKey: boolean; altKey: boolean; shiftKey: boolean }> = {}) =>
  ({ key, code: "", ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...more });

test("an accelerator matches whatever the case, with Cmd on macOS and Ctrl elsewhere", () => {
  expect(matchesAccelerator(press("q", { ctrlKey: true }), "CmdOrCtrl+Q", false)).toBe(true);
  // Caps Lock reports an upper-case key.
  expect(matchesAccelerator(press("Q", { ctrlKey: true }), "CmdOrCtrl+Q", false)).toBe(true);
  expect(matchesAccelerator(press("Q", { metaKey: true }), "CmdOrCtrl+Q", true)).toBe(true);
  // macOS quits with ⌘Q only; Ctrl+K there is "delete to end of line".
  expect(matchesAccelerator(press("q", { ctrlKey: true }), "CmdOrCtrl+Q", true)).toBe(false);
  expect(matchesAccelerator(press("k", { ctrlKey: true }), "CmdOrCtrl+K", true)).toBe(false);
  expect(matchesAccelerator(press("q", { metaKey: true }), "CmdOrCtrl+Q", false)).toBe(false);
  expect(matchesAccelerator(press("q", { metaKey: true, ctrlKey: true }), "CmdOrCtrl+Q", true)).toBe(false);
  // Extra modifiers make another shortcut.
  expect(matchesAccelerator(press("Q", { ctrlKey: true, shiftKey: true }), "CmdOrCtrl+Q", false)).toBe(false);
  expect(matchesAccelerator(press("q", { ctrlKey: true, altKey: true }), "CmdOrCtrl+Q", false)).toBe(false);
  expect(matchesAccelerator(press("q"), "CmdOrCtrl+Q", false)).toBe(false);
  expect(matchesAccelerator(press("w", { ctrlKey: true }), "CmdOrCtrl+Q", false)).toBe(false);
});

test("an accelerator falls back to the physical key only for non-Latin letters and their layout punctuation", () => {
  // Russian and Greek layouts type й and ; on the Q key.
  expect(matchesAccelerator(press("й", { code: "KeyQ", ctrlKey: true }), "CmdOrCtrl+Q", false)).toBe(true);
  expect(matchesAccelerator(press("Й", { code: "KeyQ", metaKey: true }), "CmdOrCtrl+Q", true)).toBe(true);
  expect(matchesAccelerator(press("κ", { code: "KeyK", ctrlKey: true }), "CmdOrCtrl+K", false)).toBe(true);
  // Greek and Hebrew type ASCII punctuation on the Q key: ; and /.
  expect(matchesAccelerator(press(";", { code: "KeyQ", ctrlKey: true }), "CmdOrCtrl+Q", false)).toBe(true);
  expect(matchesAccelerator(press("/", { code: "KeyQ", ctrlKey: true }), "CmdOrCtrl+Q", false)).toBe(true);
  expect(matchesAccelerator(press("/", { code: "KeyQ", metaKey: true }), "CmdOrCtrl+Q", true)).toBe(true);
  // Punctuation on its own key is never a letter shortcut.
  expect(matchesAccelerator(press(";", { code: "Semicolon", ctrlKey: true }), "CmdOrCtrl+Q", false)).toBe(false);
  expect(matchesAccelerator(press("/", { code: "Slash", ctrlKey: true }), "CmdOrCtrl+Q", false)).toBe(false);
  // A Latin layout means what it types: AZERTY's Q key types "a" and its A key
  // types "q"; Dvorak's X key types "q" and its Q key types "'".
  expect(matchesAccelerator(press("a", { code: "KeyQ", ctrlKey: true }), "CmdOrCtrl+Q", false)).toBe(false);
  expect(matchesAccelerator(press("q", { code: "KeyA", ctrlKey: true }), "CmdOrCtrl+Q", false)).toBe(true);
  expect(matchesAccelerator(press("q", { code: "KeyX", ctrlKey: true }), "CmdOrCtrl+Q", false)).toBe(true);
  expect(matchesAccelerator(press("x", { code: "KeyB", ctrlKey: true }), "CmdOrCtrl+B", false)).toBe(false);
  // Dvorak's Q key types ', so Ctrl+' is not Quit (#742); Hebrew types ' on W.
  expect(matchesAccelerator(press("'", { code: "KeyQ", ctrlKey: true }), "CmdOrCtrl+Q", false)).toBe(false);
  expect(matchesAccelerator(press("'", { code: "KeyQ", metaKey: true }), "CmdOrCtrl+Q", true)).toBe(false);
  expect(matchesAccelerator(press(",", { code: "KeyW", ctrlKey: true }), "CmdOrCtrl+W", false)).toBe(false);
  expect(matchesAccelerator(press("'", { code: "KeyW", ctrlKey: true }), "CmdOrCtrl+W", false)).toBe(true);
  // Vowel signs and non-ASCII punctuation on letter keys count too: Hindi ौ on Q, Arabic ؛ on Shift+P.
  expect(matchesAccelerator(press("ौ", { code: "KeyQ", ctrlKey: true }), "CmdOrCtrl+Q", false)).toBe(true);
  expect(matchesAccelerator(press("؛", { code: "KeyP", ctrlKey: true, shiftKey: true }), "CmdOrCtrl+Shift+P", false)).toBe(true);
  // Digits still follow the physical key, as on AZERTY's digit row.
  expect(matchesAccelerator(press("&", { code: "Digit1", ctrlKey: true }), "CmdOrCtrl+1", false)).toBe(true);
});
