import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { FileDrag } from "./FileDrag";

afterEach(cleanup);

test("the shared file source delegates a native drag and keeps click and keyboard access to its alternative", () => {
  const onDrag = vi.fn();
  const onPress = vi.fn();
  render(<FileDrag icon="/invented-icon.png" label="Drag Ibis" description="Press to open System Settings." onDrag={onDrag} onPress={onPress} />);
  const source = screen.getByRole("button", { name: "Drag Ibis" });
  expect(source.getAttribute("draggable")).toBe("true");
  expect(source.getAttribute("aria-description")).toBe("Press to open System Settings.");
  expect(source.querySelector("img")?.draggable).toBe(false);
  fireEvent.click(source);
  expect(onPress).toHaveBeenCalledOnce();
  expect(onDrag).not.toHaveBeenCalled();
  expect(fireEvent.dragStart(source)).toBe(false);
  expect(onDrag).toHaveBeenCalledOnce();
  // A cancelled native drag must not open Settings when the pointer is released.
  fireEvent.click(source, { detail: 1 });
  expect(onPress).toHaveBeenCalledOnce();
  source.focus();
  fireEvent.keyDown(source, { key: "Enter" });
  fireEvent.keyUp(source, { key: "Enter" });
  expect(onPress).toHaveBeenCalledTimes(2);
  fireEvent.pointerDown(source, { button: 0, pointerType: "mouse" });
  fireEvent.click(source);
  expect(onPress).toHaveBeenCalledTimes(3);
});

test("assistive-technology activation still opens the alternative after a cancelled drag", () => {
  const onPress = vi.fn();
  render(<FileDrag icon="/invented-icon.png" label="Drag Ibis" description="Open Settings" onDrag={vi.fn()} onPress={onPress} />);
  const source = screen.getByRole("button", { name: "Drag Ibis" });
  fireEvent.dragStart(source);
  fireEvent.click(source, { detail: 0 });
  expect(onPress).toHaveBeenCalledOnce();
});
