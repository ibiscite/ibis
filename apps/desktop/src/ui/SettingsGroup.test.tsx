import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { Row } from "./Layout";

afterEach(cleanup);

test("a changed setting offers a named default in a slot an unchanged setting keeps empty", () => {
  const reset = vi.fn();
  const view = render(<Row id="appearance" label="Appearance" reset={{ label: "Appearance", onReset: reset }}>Light</Row>);
  fireEvent.click(screen.getByRole("button", { name: "Use Default for Appearance" }));
  expect(reset).toHaveBeenCalledOnce();
  view.rerender(<Row id="appearance" label="Appearance">System</Row>);
  expect(screen.queryByRole("button", { name: "Use Default for Appearance" })).toBeNull();
  expect(view.container.querySelector(".ui-row-reset")).not.toBeNull();
});
