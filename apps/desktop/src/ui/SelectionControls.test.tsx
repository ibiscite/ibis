import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { Segmented } from "./Segmented";
import { Switch } from "./Switch";

const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
const css = ["Switch.css", "Segmented.css"].map((file) => readFileSync(new FileURL(file, import.meta.url), "utf8")).join("\n");

afterEach(cleanup);

function Controls() {
  const [enabled, setEnabled] = useState(false);
  const [rows, setRows] = useState("comfortable");
  return <section style={{ position: "relative" }}>
    <style>{css}</style>
    <Switch aria-label="Show invented preview" isSelected={enabled} onChange={setEnabled} />
    <Segmented label="Invented rows" value={rows} onChange={setRows}
      options={[{ value: "compact", label: "Compact" }, { value: "comfortable", label: "Comfortable" }, { value: "spacious", label: "Spacious" }]} />
  </section>;
}

function containingBlock(node: HTMLElement) {
  for (let parent = node.parentElement; parent; parent = parent.parentElement) {
    const position = getComputedStyle(parent).position;
    if (position && position !== "static") return parent;
  }
  return null;
}

test.each(["switch", "radio"])("each %s keeps its hidden input positioned by its own associated label", (role) => {
  render(<Controls />);
  for (const input of screen.getAllByRole(role)) {
    const hidden = input.parentElement!;
    const label = input.closest("label")!;
    expect(label.control).toBe(input);
    expect(getComputedStyle(hidden).position).toBe("absolute");
    expect(containingBlock(hidden)).toBe(label);
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
  expect(document.activeElement).toBe(screen.getByRole("radio", { name: "Spacious", checked: true }));
  expect(screen.getByRole("radiogroup", { name: "Invented rows" })).toBeTruthy();
});

test("a disabled switch keeps its native input disabled and refuses activation", () => {
  const change = vi.fn();
  render(<><style>{css}</style><Switch aria-label="Unavailable invented option" isDisabled isSelected={false} onChange={change} /></>);
  const toggle = screen.getByRole("switch", { name: "Unavailable invented option" }) as HTMLInputElement;
  act(() => toggle.click());
  expect(change).not.toHaveBeenCalled();
  expect(toggle.checked).toBe(false);
  expect(toggle.disabled).toBe(true);
});
