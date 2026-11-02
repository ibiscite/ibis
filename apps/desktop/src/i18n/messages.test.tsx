import { pickerText } from "../messages";
import { render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import english from "./en-US.ftl?raw";
import { createInterfaceMessages, englishMessages, InterfaceMessagesProvider, type MessageId, useInterfaceMessages } from ".";
import { loginItemText } from "../messages";

test("the bundled interface default is English, independent of browser language and direction", () => {
  expect(englishMessages.locale).toBe("en-US");
  expect(englishMessages.text("settings-general")).toBe("General");
  expect(englishMessages.text("general-login-label", { platform: "windows" })).toBe("Start when you sign in");
  expect(englishMessages.text("general-login-label", { platform: "macos" })).toBe("Open at Login");
  expect(englishMessages.text("general-login-label", { platform: "linux" })).toBe("Start on Login");
  expect(Object.isFrozen(englishMessages)).toBe(true);
});

test("English login status messages preserve the existing platform wording", () => {
  for (const platform of ["macos", "windows", "linux"] as const) {
    for (const [problem, id] of [["unsupported", "general-login-unsupported"], ["needs_approval", "general-login-needs-approval"],
      ["refused", "general-login-refused"], ["unreadable", "general-login-unreadable"]] as const) {
      expect(englishMessages.text(id, { platform, name: "Synthetic App" })).toBe(loginItemText.problem(problem, platform, "Synthetic App"));
    }
  }
});

test.each([
  "", english.replace(/^settings-general =.*$/m, ""),
  english + "\nbroken = {\n", english + "\nsettings-general = Duplicate\n",
  english.replace("settings-general = General", "settings-general = { missing-message }"),
  english.replace("settings-general = General", "settings-general = { $unexpected }"),
  english.replace(/^shortcuts-open =.*$/m, ""),
  english.replace(/^about-check =.*$/m, "about-check = { missing-message }"),
  english.replace(/^shortcuts-picker-conflict =.*$/m, "shortcuts-picker-conflict = { $unexpected }"),
])("missing or invalid bundled resources refuse the whole message owner", (source) => {
  expect(() => createInterfaceMessages(source)).toThrow("Invalid interface messages");
});

// Frozen pre-migration English oracle, including punctuation and capitalization.
const panelEnglish = {
  "shortcuts-open": "Open Ibis",
  "shortcuts-description": "Click the shortcut, then type a new one. Esc cancels.",
  "shortcuts-record": "Type a shortcut…",
  "shortcuts-use-default": "Use Default",
  "shortcuts-change": "Change…",
  "shortcuts-not-assigned": "Not assigned",
  "shortcuts-configure-failed": "The desktop could not open its shortcut settings. Try again.",
  "shortcuts-failed": "Ibis could not change the shortcut. Try again.",
  "shortcuts-read-failed": "Ibis could not check the shortcut. Try reopening Settings.",
  "shortcuts-refusal-invalid": "Ibis cannot use that key combination. Try another shortcut.",
  "shortcuts-refusal-needs-modifier": "Include Control, Alt or Command. Shift alone is not enough.",
  "shortcuts-refusal-app-shortcut": "That shortcut is used for app commands. Add another modifier.",
  "shortcuts-refusal-alt-gr": "Ctrl+Alt with a typing key is used for AltGr on Windows. Try another shortcut.",
  "shortcuts-refusal-in-use": "Another app or the system may be using that shortcut. Try another.",
  "shortcuts-refusal-unavailable": "Global shortcuts are unavailable on this desktop. Open Ibis from your application launcher.",
  "shortcuts-refusal-not-saved": "The shortcut works now, but Ibis could not save it for next time. Try again.",
  "shortcuts-command-format": "Choose Output Format",
  "shortcuts-command-pin": "Keep Window Open",
  "shortcuts-command-keys": "Keyboard Shortcuts",
  "shortcuts-command-more": "More",
  "shortcuts-command-source": "Open the Source",
  "shortcuts-command-preview": "Show What Will Be Pasted",
  "shortcuts-command-settings": "Settings",
  "shortcuts-command-quit": "Quit Ibis",
  "shortcuts-region-search": "While searching",
  "shortcuts-region-library": "Your library",
  "shortcuts-region-picker": "Anywhere in Ibis",
  "shortcuts-region-menu": "In a menu",
  "shortcuts-region-system": "From any app",
  "shortcuts-action-choose-source": "Choose a library",
  "shortcuts-action-reveal-source": "Show the source in its folder",
  "shortcuts-action-source": "Open the source",
  "shortcuts-action-preview": "Show what will be pasted",
  "shortcuts-action-copy": "Paste the citation",
  "shortcuts-action-copy-only": "Copy the citation",
  "shortcuts-action-add": "Add another reference",
  "shortcuts-action-move": "Move through results",
  "shortcuts-action-escape": "Step back or hide",
  "shortcuts-action-format": "Choose the format",
  "shortcuts-action-more": "More actions",
  "shortcuts-action-pin": "Keep the window open",
  "shortcuts-action-keys": "Show this list",
  "shortcuts-action-settings": "Open Settings",
  "shortcuts-action-quit": "Quit Ibis",
  "shortcuts-action-menu-move": "Move through the menu",
  "shortcuts-action-menu-ends": "Go to the first or last item",
  "shortcuts-action-menu-choose": "Choose the item",
  "shortcuts-action-menu-leave": "Leave the menu and return to the search",
  "shortcuts-or": "or",
  "about-automatic": "Check for updates automatically",
  "about-automatic-description": "Reads a small update file from GitHub a few times a day",
  "about-check": "Check now",
  "about-copied": "Copied",
  "about-copy-failed": "The version could not be copied.",
  "about-unavailable": "This build does not update itself.",
} as const;

test.each(Object.entries(panelEnglish))("%s preserves exact English and expands in the pseudolocale", (id, expected) => {
  expect(englishMessages.text(id as MessageId)).toBe(expected);
  expect(createInterfaceMessages(english, true).text(id as MessageId)).toMatch(/^⟦.+⟧$/);
});

test("panel variables preserve exact English and formatting errors refuse without partial copy", () => {
  expect(englishMessages.text("about-version", { name: "Synthetic App", version: "0.3.0" })).toBe("Version 0.3.0");
  expect(englishMessages.text("about-version", { name: "Ibis", version: "" })).toBe("Version ");
  expect(englishMessages.text("shortcuts-picker-conflict", { command: "Choose Output Format" })).toBe("That shortcut is used for Choose Output Format. Try another shortcut.");
  for (const id of ["about-version", "shortcuts-picker-conflict"] as const) expect(() => englishMessages.text(id)).toThrow("Invalid interface message");
  const child = vi.fn(() => <p>Partial panel</p>);
  expect(() => render(<InterfaceMessagesProvider messages={createInterfaceMessages(english.replace(/^about-check =.*$/m, ""))}>{child()}</InterfaceMessagesProvider>)).toThrow("Invalid interface messages");
  expect(child).not.toHaveBeenCalled();
  expect(screen.queryByText("Partial panel")).toBeNull();
});

function Label() {
  return <p>{useInterfaceMessages().text("general-paste-drag", { name: '<img src=x onerror="throw 1">', list: "{ $secret } & <b>List</b>" })}</p>;
}

test("hostile external values remain literal text without markup or Fluent evaluation", () => {
  render(<InterfaceMessagesProvider messages={englishMessages}><Label /></InterfaceMessagesProvider>);
  expect(screen.getByText('Drag <img src=x onerror="throw 1"> to { $secret } & <b>List</b>')).toBeTruthy();
  expect(document.querySelector("img, b")).toBeNull();
  expect(() => englishMessages.text("general-paste-drag")).toThrow("Invalid interface message");
});

test("the pseudolocale marks and expands interface text while keeping external values exact", () => {
  const pseudo = createInterfaceMessages(english, true);
  expect(pseudo.text("settings-general")).toMatch(/^⟦.+⟧$/);
  expect(pseudo.text("settings-general").length).toBeGreaterThan("General".length * 1.3);
  expect(pseudo.text("general-paste-drag", { name: "Synthetic App", list: "Synthetic List" })).toContain("Synthetic App");
  expect(pseudo.text("general-paste-drag", { name: "Synthetic App", list: "Synthetic List" })).toContain("Synthetic List");
});

test("settings wording follows the platform, and a backend reason ends on one full stop", () => {
  const says = (id: Parameters<typeof englishMessages.text>[0], platform: string) => englishMessages.text(id, { platform });
  expect(says("general-paste-description", "macos")).toBe("Otherwise Ibis copies and you press ⌘V");
  for (const platform of ["windows", "linux"]) {
    expect(says("general-paste-description", platform)).toBe("Otherwise Ibis copies and you press Ctrl+V");
    expect(says("look-accent-system", platform)).toBe("Follow the system");
    expect(says("look-accent-multicolor", platform)).not.toContain("Mac");
  }
  expect(says("look-accent-system", "macos")).toBe("Follow the Mac");
  expect(pickerText.shortcutError(false, "Ctrl+Shift+Y", "Restart Ibis after the desktop portal is available."))
    .toBe("The shortcut Ctrl+Shift+Y is not available: Restart Ibis after the desktop portal is available.");
  expect(pickerText.shortcutError(true, "Ctrl+Shift+Y", "it is taken")).toBe("Your shortcut setting was not used: it is taken.");
});
