import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import english from "./en-US.ftl?raw";
import { createInterfaceMessages, englishMessages, InterfaceMessagesProvider, useInterfaceMessages } from ".";
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
])("missing or invalid bundled resources refuse the whole message owner", (source) => {
  expect(() => createInterfaceMessages(source)).toThrow("Invalid interface messages");
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
