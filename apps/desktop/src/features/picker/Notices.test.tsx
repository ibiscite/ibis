import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Notices } from "./Notices";

afterEach(cleanup);
const copied = { text: "[@example2026]", warnings: ["Missing year — 研究 العربية <literal>"] };
const props = { library: null, failures: {}, startAtLogin: null, error: null, copyError: null,
  copied, showCopiedText: true, onDismissCopied: vi.fn() };

test.each(["Enter", " "])("warning dismissal uses shared states and activates once with %s", (key) => {
  const dismiss = vi.fn();
  render(<Notices {...props} onDismissCopied={dismiss} />);
  const button = screen.getByRole("button", { name: "Dismiss copy warnings" });
  expect(button.getAttribute("data-variant")).toBe("secondary");
  expect(button.getAttribute("data-size")).toBe("default");
  expect(screen.getByRole("status").textContent).toContain(copied.text);
  expect(screen.getByLabelText("Copy warnings").textContent).toBe(copied.warnings[0]);
  button.focus();
  const code = key === " " ? "Space" : "Enter";
  fireEvent.keyDown(button, { key, code });
  fireEvent.keyDown(button, { key, code, repeat: true });
  fireEvent.keyUp(button, { key, code });
  expect(dismiss).toHaveBeenCalledTimes(1);
});

test("clean confirmations have no dismissal action or warning list", () => {
  render(<Notices {...props} copied={{ ...copied, warnings: [] }} />);
  expect(screen.getByRole("status")).toBeTruthy();
  expect(screen.queryByRole("button")).toBeNull();
  expect(screen.queryByLabelText("Copy warnings")).toBeNull();
});
