import { afterEach, expect, test } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useAltHeld } from "./useAltHeld";

afterEach(() => { cleanup(); delete document.documentElement.dataset.system; });
function Example() {
  const { altHeld } = useAltHeld();
  return <><input aria-label="Search" /><footer><button>{altHeld ? "Alternate" : "Open"}</button></footer></>;
}

test.each(["windows", "linux"])("%s: Shift shows alternates over the footer, leaving shifted search typing alone", (platform) => {
  document.documentElement.dataset.system = platform;
  render(<Example />);
  const field = screen.getByRole("textbox");
  field.focus();
  fireEvent.keyDown(field, { key: "Shift", shiftKey: true });
  expect(screen.getByRole("button", { name: "Open" })).toBeTruthy();
  fireEvent.keyDown(field, { key: "A", shiftKey: true });
  fireEvent.change(field, { target: { value: "A" } });
  expect((field as HTMLInputElement).value).toBe("A");
  expect(document.activeElement).toBe(field);
  fireEvent.keyUp(field, { key: "Shift" });
  fireEvent.pointerOver(screen.getByRole("button"));
  fireEvent.keyDown(field, { key: "Alt", altKey: true });
  expect(screen.getByRole("button", { name: "Open" })).toBeTruthy();
  fireEvent.keyUp(field, { key: "Alt" });
  fireEvent.keyDown(field, { key: "Shift", shiftKey: true });
  expect(screen.getByRole("button", { name: "Alternate" })).toBeTruthy();
  fireEvent.pointerOut(screen.getByRole("button"), { relatedTarget: field });
  expect(screen.getByRole("button", { name: "Open" })).toBeTruthy();
  // A release outside this window may have been missed; the pointer reports
  // the current modifiers when it returns to the footer.
  fireEvent.pointerOver(screen.getByRole("button"));
  expect(screen.getByRole("button", { name: "Open" })).toBeTruthy();
  fireEvent.keyUp(field, { key: "Shift" });
});

test("macOS: Option shows alternates; another key, release or blur ends them", () => {
  document.documentElement.dataset.system = "macos";
  render(<Example />);
  fireEvent.keyDown(window, { key: "Alt", altKey: true });
  expect(screen.getByRole("button", { name: "Alternate" })).toBeTruthy();
  fireEvent.keyDown(window, { key: "e", altKey: true });
  expect(screen.getByRole("button", { name: "Open" })).toBeTruthy();
  fireEvent.keyUp(window, { key: "Alt" });
  fireEvent.keyDown(window, { key: "Alt", altKey: true });
  fireEvent.blur(window);
  expect(screen.getByRole("button", { name: "Open" })).toBeTruthy();
});
