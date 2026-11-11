import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { Row } from "./Layout";

afterEach(cleanup);

test("a changed setting offers a named reset and an unchanged setting does not", () => {
  const reset = vi.fn();
  const view = render(<Row id="appearance" label="Appearance" changed onReset={reset}>Light</Row>);
  expect(screen.getByText("Changed")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Reset Appearance" }));
  expect(reset).toHaveBeenCalledOnce();
  view.rerender(<Row id="appearance" label="Appearance" changed={false} onReset={reset}>System</Row>);
  expect(screen.queryByRole("button", { name: "Reset Appearance" })).toBeNull();
});
