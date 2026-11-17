import { afterEach, expect, test } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useAltHeld } from "./useAltHeld";

afterEach(() => { cleanup(); delete document.documentElement.dataset.system; });
function Example() {
  const { altHeld } = useAltHeld();
  return <><input aria-label="Search" /><footer><button>{altHeld ? "Alternate" : "Open"}</button></footer></>;
}

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
