import { expect, test } from "vitest";
import { ariaKeys, KEY_TABLE, keyLabel, matchesAccelerator, shortcutLabel } from "./keys";
import { keyText } from "./messages";

test("key labels follow the platform", () => {
  expect(["CmdOrCtrl+K", "Shift+Enter", "CmdOrCtrl+/", "Escape", "ArrowDown", "?"].map((keys) => keyLabel(keys, true)))
    .toEqual(["⌘K", "⇧↵", "⌘/", "Esc", "↓", "?"]);
  expect(["CmdOrCtrl+K", "Shift+Enter", "CmdOrCtrl+/", "Escape", "ArrowDown", "?"].map((keys) => keyLabel(keys, false)))
    .toEqual(["Ctrl+K", "Shift+Enter", "Ctrl+/", "Esc", "↓", "?"]);
  expect(keyLabel("CmdOrCtrl++", false)).toBe("Ctrl++");
  expect(ariaKeys("CmdOrCtrl+K", true)).toBe("Meta+K");
  expect(ariaKeys("CmdOrCtrl+K", false)).toBe("Control+K");
});

test("every key in the table has words and a region, and none is listed twice", () => {
  const seen = new Set<string>();
  for (const binding of KEY_TABLE) {
    expect(keyText.actions[binding.action]).toBeTruthy();
    expect(keyText.regions[binding.region]).toBeTruthy();
    for (const keys of binding.keys) {
      const where = `${binding.region}:${keys}`;
      expect(seen.has(where), where).toBe(false);
      seen.add(where);
    }
  }
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

test("an accelerator falls back to the physical key only when the layout types a non-Latin letter", () => {
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
});

test("every letter accelerator in the table can be matched", () => {
  for (const binding of KEY_TABLE) {
    for (const keys of binding.keys) {
      const letter = /^CmdOrCtrl\+([A-Z])$/.exec(keys)?.[1];
      if (!letter) continue;
      expect(matchesAccelerator(press(letter.toLowerCase(), { ctrlKey: true }), keys, false), keys).toBe(true);
      expect(matchesAccelerator(press(letter, { metaKey: true }), keys, true), keys).toBe(true);
    }
  }
});

test("shortcut labels read settings in any case and alias", () => {
  expect(shortcutLabel("Control+Command+R", true)).toBe("⌃⌘R");
  expect(shortcutLabel("cmdorctrl + shift + keyr", false)).toBe("Ctrl+Shift+R");
});
