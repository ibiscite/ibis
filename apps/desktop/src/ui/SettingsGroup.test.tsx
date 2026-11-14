import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { Row } from "./Layout";

afterEach(cleanup);

test("a changed setting offers a named default in a slot an unchanged setting keeps empty", () => {
  const reset = vi.fn();
  const view = render(<Row id="appearance" label="Appearance" reset={null}>System</Row>);
  const slot = view.container.querySelector(".ui-row-reset");
  expect(slot).not.toBeNull();
  expect(slot?.childElementCount).toBe(0);
  view.rerender(<Row id="appearance" label="Appearance" reset={{ label: "Appearance", onReset: reset }}>Light</Row>);
  fireEvent.click(screen.getByRole("button", { name: "Use Default for Appearance" }));
  expect(reset).toHaveBeenCalledOnce();
  view.rerender(<Row id="appearance" label="Appearance" reset={null}>System</Row>);
  expect(screen.queryByRole("button", { name: "Use Default for Appearance" })).toBeNull();
  expect(view.container.querySelector(".ui-row-reset")).toBe(slot);
});

test.each([false, true])("a non-resettable row has no reset slot (hidden label: %s)", (labelHidden) => {
  const { container } = render(<Row label="Back up settings" labelHidden={labelHidden}><button type="button">Back Up</button></Row>);
  expect(container.querySelector(".ui-row-reset")).toBeNull();
});
