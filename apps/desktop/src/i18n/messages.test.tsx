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
  "shortcuts-in-picker": "In the picker",
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
  "shortcuts-command-preview": "Show What Will Be Pasted",
  "shortcuts-command-settings": "Settings",
  "shortcuts-command-quit": "Quit Ibis",
  "shortcuts-region-search": "In the search box",
  "shortcuts-region-picker": "Anywhere in Ibis",
  "shortcuts-region-menu": "In a menu",
  "shortcuts-region-system": "From any app",
  "shortcuts-action-preview": "Show what will be pasted",
  "shortcuts-action-copy": "Paste or copy the highlighted reference, or the ones added to the citation, as the footer says",
  "shortcuts-action-add": "Add the highlighted reference to the citation, or remove it",
  "shortcuts-action-move": "Move through the results",
  "shortcuts-action-escape": "Step back: close a menu, clear the search, then hide Ibis",
  "shortcuts-action-format": "Choose the output format and where to remember it",
  "shortcuts-action-more": "Open or close More",
  "shortcuts-action-pin": "Keep Window Open, or stop keeping it open",
  "shortcuts-action-keys": "Show this list (? works outside the search box)",
  "shortcuts-action-settings": "Open Settings",
  "shortcuts-action-quit": "Quit Ibis",
  "shortcuts-action-menu-move": "Move through the menu",
  "shortcuts-action-menu-ends": "Go to the first or last item",
  "shortcuts-action-menu-choose": "Choose the item",
  "shortcuts-action-menu-leave": "Leave the menu and return to the search",
  "shortcuts-or": "or",
  "about-updates": "Updates",
  "about-automatic": "Check for updates automatically",
  "about-automatic-description": "Reads a small update file from GitHub a few times a day",
  "about-check": "Check now",
  "about-unavailable": "This build does not update itself.",
} as const;

test.each(Object.entries(panelEnglish))("%s preserves exact English and expands in the pseudolocale", (id, expected) => {
  expect(englishMessages.text(id as MessageId)).toBe(expected);
  expect(createInterfaceMessages(english, true).text(id as MessageId)).toMatch(/^⟦.+⟧$/);
});

test("panel variables preserve exact English and formatting errors refuse without partial copy", () => {
  expect(englishMessages.text("about-version", { name: "Synthetic App", version: "0.3.0" })).toBe("Synthetic App 0.3.0");
  expect(englishMessages.text("about-version", { name: "Ibis", version: "" })).toBe("Ibis ");
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
