import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { Row } from "./Layout";
import { settingControl } from "./settingFocus";

afterEach(cleanup);

test("Use Default follows the setting name, stays reachable and leaves no slot at the default", () => {
  const reset = vi.fn();
  const view = render(<Row id="appearance" label="Appearance" reset={{ label: "Appearance", onReset: reset }}><button>Light</button></Row>);
  const restore = screen.getByRole("button", { name: "Use Default for Appearance" });
  const control = screen.getByRole("button", { name: "Light" });
  expect(restore.tabIndex).toBe(0);
  expect(screen.getByText("Appearance").nextElementSibling?.contains(restore)).toBe(true);
  expect([...view.container.querySelectorAll("button")]).toEqual([restore, control]);
  fireEvent.click(restore);
  expect(reset).toHaveBeenCalledOnce();
  expect(document.activeElement).toBe(control);
  view.rerender(<Row id="appearance" label="Appearance">System</Row>);
  expect(screen.queryByRole("button", { name: "Use Default for Appearance" })).toBeNull();
  expect(view.container.querySelector(".ui-row-reset")).toBeNull();
});

test("a settings search jump skips Use Default, including while its control is disabled", () => {
  const view = render(<Row label="Opening shortcut" reset={{ label: "Opening shortcut", onReset: vi.fn() }}><button>Record shortcut</button></Row>);
  const row = view.container.firstElementChild as HTMLElement;
  const control = screen.getByRole("button", { name: "Record shortcut" });
  act(() => { settingControl(row)?.focus(); });
  expect(document.activeElement).toBe(control);
  control.setAttribute("disabled", "");
  expect(settingControl(row)).toBeNull();
});
