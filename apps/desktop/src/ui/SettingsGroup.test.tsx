import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { Row } from "./Layout";
import { settingControl } from "./settingFocus";

afterEach(cleanup);

test("Use Default sits right before its control, in room a resettable row keeps at the default", () => {
  const reset = vi.fn();
  const view = render(<Row id="appearance" label="Appearance" reset={{ label: "Appearance", onReset: reset }}><button>Light</button></Row>);
  const restore = screen.getByRole("button", { name: "Use Default for Appearance" });
  const control = screen.getByRole("button", { name: "Light" });
  const slot = restore.closest(".ui-row-reset")!;
  expect(restore.tabIndex).toBe(0);
  expect(slot.nextElementSibling).toBe(control);
  expect([...view.container.querySelectorAll("button")]).toEqual([restore, control]);
  fireEvent.click(restore);
  expect(reset).toHaveBeenCalledOnce();
  expect(document.activeElement).toBe(control);
  view.rerender(<Row id="appearance" label="Appearance" reset={null}><button>System</button></Row>);
  expect(screen.queryByRole("button", { name: "Use Default for Appearance" })).toBeNull();
  expect(view.container.querySelector(".ui-row-reset")).toBe(slot);
  expect(slot.childElementCount).toBe(0);
});

test.each([false, true])("a row that cannot reset keeps no room for it (hidden label: %s)", (labelHidden) => {
  const { container } = render(<Row label="Back up settings" labelHidden={labelHidden}><button type="button">Back Up</button></Row>);
  expect(container.querySelector(".ui-row-reset")).toBeNull();
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
