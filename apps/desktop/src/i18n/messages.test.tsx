import { render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import english from "../../../../locales/en-US.ftl?raw";
import { createInterfaceMessages, englishMessages, InterfaceMessagesProvider, useInterfaceMessages } from ".";

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
