import { render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import english from "../../../../locales/en-US.ftl?raw";
import { createInterfaceMessages, englishMessages, fullStop, InterfaceMessagesProvider, say, useInterfaceMessages } from ".";

test("the bundled interface default is English, independent of browser language and direction", () => {
  expect(englishMessages.locale).toBe("en-US");
  expect(englishMessages.text("settings-general")).toBe("General");
  expect(englishMessages.text("general-login-label", { platform: "windows" })).toBe("Start when you sign in");
  expect(englishMessages.text("general-login-label", { platform: "macos" })).toBe("Open at login");
  expect(englishMessages.text("general-login-label", { platform: "linux" })).toBe("Start on login");
  expect(Object.isFrozen(englishMessages)).toBe(true);
});

test("interface messages resolve a shared Fluent term", () => {
  const source = english + "\n-library-formats-test = BibTeX, BibLaTeX, CSL-JSON, RIS or CSV\n";
  const messages = createInterfaceMessages(source.replace(/^choice-library-file-help =.*$/m,
    "choice-library-file-help = { -library-formats-test }"));
  expect(messages.text("choice-library-file-help")).toBe("BibTeX, BibLaTeX, CSL-JSON, RIS or CSV");
});

test.each([
  "", english.replace(/^settings-general =.*$/m, ""),
  english + "\nbroken = {\n", english + "\nsettings-general = Duplicate\n",
  english.replace("settings-general = General", "settings-general = { missing-message }"),
  english.replace("settings-general = General", "settings-general = { $unexpected }"),
  english.replace(/^shortcuts-open =.*$/m, ""),
  english.replace(/^about-check =.*$/m, "about-check = { missing-message }"),
  english.replace(/^shortcuts-picker-conflict =.*$/m, "shortcuts-picker-conflict = { $unexpected }"),
  english.replace(/^-library-formats =.*$/m, ""),
  english + "\n-library-formats = Duplicate\n",
])("missing or invalid bundled resources refuse the whole message owner", (source) => {
  expect(() => createInterfaceMessages(source)).toThrow("Invalid interface messages");
});

test("missing variables and invalid catalogues refuse without partial rendering", () => {
  expect(englishMessages.text("about-version", { name: "Synthetic App", version: "0.3.0" })).toBe("Version 0.3.0");
  expect(englishMessages.text("about-version", { name: "Ibis", version: "" })).toBe("Version ");
  expect(englishMessages.text("shortcuts-picker-conflict", { command: "Change Format" })).toBe("That shortcut is used for Change Format. Try another shortcut.");
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
  expect(says("general-paste-description", "macos")).toBe("Otherwise Ibis copies and you press ⌘V.");
  for (const platform of ["windows", "linux"]) {
    expect(says("general-paste-description", platform)).toBe("Otherwise Ibis copies and you press Ctrl+V.");
    expect(says("look-accent-system", platform)).toBe("Follow the system");
  }
  expect(says("look-accent-system", "macos")).toBe("Follow the Mac");
  expect(say("picker-shortcut-unavailable", { shortcut: "Ctrl+Shift+Y", reason: fullStop("Restart Ibis after the desktop portal is available.") }))
    .toBe("The shortcut Ctrl+Shift+Y is not available: Restart Ibis after the desktop portal is available.");
  expect(say("picker-shortcut-not-used", { reason: fullStop("it is taken") })).toBe("Your shortcut setting was not used: it is taken.");
});
