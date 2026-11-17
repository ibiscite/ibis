import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { Row } from "./Layout";

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
