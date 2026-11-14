import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { Segmented } from "./Segmented";
import { Switch } from "./Switch";

afterEach(cleanup);

function Controls() {
  const [enabled, setEnabled] = useState(false);
  const [rows, setRows] = useState("comfortable");
  return <section style={{ position: "relative" }}>
    <Switch aria-label="Show invented preview" isSelected={enabled} onChange={setEnabled} />
    <Segmented label="Invented rows" value={rows} onChange={setRows}
      options={[{ value: "compact", label: "Compact" }, { value: "comfortable", label: "Comfortable" }, { value: "wide", label: "Wide" }]} />
  </section>;
}

test.each(["switch", "radio"])("each %s keeps its hidden input inside its own associated label", (role) => {
  render(<Controls />);
  for (const input of screen.getAllByRole(role)) {
    const hidden = input.parentElement!;
    const label = input.closest("label")!;
    expect(label.control).toBe(input);
    expect(hidden.style.position).toBe("absolute");
  }
});

test("switch activation and segmented arrow keys preserve names and controlled values", () => {
  render(<Controls />);
  const toggle = screen.getByRole("switch", { name: "Show invented preview" }) as HTMLInputElement;
  act(() => toggle.focus());
  act(() => toggle.click());
  expect(toggle.checked).toBe(true);
  expect(document.activeElement).toBe(toggle);
  const comfortable = screen.getByRole("radio", { name: "Comfortable", checked: true });
  act(() => comfortable.focus());
  fireEvent.keyDown(comfortable, { key: "ArrowRight" });
  expect(document.activeElement).toBe(screen.getByRole("radio", { name: "Wide", checked: true }));
  expect(screen.getByRole("radiogroup", { name: "Invented rows" })).toBeTruthy();
});

test("a disabled switch keeps its native input disabled and refuses activation", () => {
  const change = vi.fn();
  render(<Switch aria-label="Unavailable invented option" isDisabled isSelected={false} onChange={change} />);
  const toggle = screen.getByRole("switch", { name: "Unavailable invented option" }) as HTMLInputElement;
  act(() => toggle.click());
  expect(change).not.toHaveBeenCalled();
  expect(toggle.checked).toBe(false);
  expect(toggle.disabled).toBe(true);
});
