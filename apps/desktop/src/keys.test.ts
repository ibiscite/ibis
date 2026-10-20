import { expect, test } from "vitest";
import { ariaKeys, KEY_TABLE, keyLabel } from "./keys";
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
