import { afterEach, expect, test } from "vitest";
import { trackInputModality } from "./inputModality";

afterEach(() => { delete document.documentElement.dataset.input; });

test("the last input kind is recorded so the format control can skip the ring after a click", () => {
  const stop = trackInputModality();
  expect(document.documentElement.dataset.input).toBe("keyboard");
  document.dispatchEvent(new Event("pointerdown"));
  expect(document.documentElement.dataset.input).toBe("pointer");
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown" }));
  expect(document.documentElement.dataset.input).toBe("keyboard");
  // Modifier keys alone (Ctrl held while clicking) do not count as keyboard use.
  document.dispatchEvent(new Event("pointerdown"));
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "Shift" }));
  expect(document.documentElement.dataset.input).toBe("pointer");
  stop();
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab" }));
  expect(document.documentElement.dataset.input).toBe("pointer");
});
