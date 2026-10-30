import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Notices } from "./Notices";

afterEach(cleanup);
const copied = { text: "[@example2026]", warnings: ["Missing year — 研究 العربية <literal>"] };
const props = { library: null, failures: {}, startAtLogin: null, error: null, copyError: null,
  copied, showCopiedText: true, onDismissCopied: vi.fn() };

test("a plain copy shows its first-copy hint, while warnings keep their meaning", () => {
  const confirmation = { text: "[@invented]", warnings: [], paste_hint: true };
  const { rerender } = render(<Notices {...props} copied={confirmation} />);
  expect(screen.getByRole("status", { name: "" }).textContent).toContain("Copied. Press Ctrl+V to paste");
  rerender(<Notices {...props} copied={{ ...confirmation, warnings: copied.warnings }} />);
  expect(screen.getByRole("status", { name: "" }).textContent).not.toContain("Press Ctrl+V");
  expect(screen.getByLabelText("Copy warnings").textContent).toBe(copied.warnings[0]);
});

test.each(["Enter", " "])("warning dismissal uses shared states and activates once with %s", (key) => {
  const dismiss = vi.fn();
  render(<Notices {...props} onDismissCopied={dismiss} />);
  const button = screen.getByRole("button", { name: "Dismiss copy warnings" });
  expect(button.getAttribute("data-variant")).toBe("secondary");
  expect(button.getAttribute("data-size")).toBe("default");
  expect(screen.getByRole("status", { name: "" }).textContent).toContain(copied.text);
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
  expect(screen.getByRole("status", { name: "" })).toBeTruthy();
  expect(screen.queryByRole("button")).toBeNull();
  expect(screen.queryByLabelText("Copy warnings")).toBeNull();
});

// Download completion is not installation success: verification still follows.
test("update progress keeps known, unknown, checking and failed phases honest", () => {
  const progress = { attempt: 1, revision: 1, received: 25, total: 100, phase: "downloading" as const };
  const { rerender } = render(<Notices {...props} copied={null} {...{ updateProgress: progress }} />);
  expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("25");
  expect(screen.getByRole("progressbar").textContent).toContain("25%");
  expect(screen.getByRole("progressbar").hasAttribute("data-indeterminate")).toBe(false);
  rerender(<Notices {...props} copied={null} {...{ updateProgress: { ...progress, total: null } }} />);
  expect(screen.getByRole("progressbar").hasAttribute("aria-valuenow")).toBe(false);
  expect(screen.getByRole("progressbar").textContent).not.toContain("%");
  expect(screen.getByRole("progressbar").hasAttribute("data-indeterminate")).toBe(true);
  rerender(<Notices {...props} copied={null} {...{ updateProgress: { ...progress, received: 100 } }} />);
  expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("100");
  expect(screen.getByRole("progressbar").querySelector<HTMLElement>(".update-track > div")?.style.width).toBe("100%");
  rerender(<Notices {...props} copied={null} {...{ updateProgress: { ...progress, received: 100, phase: "installing" } }} />);
  expect(screen.getByRole("progressbar").textContent).toContain("Checking and installing");
  expect(screen.getByRole("progressbar").hasAttribute("aria-valuenow")).toBe(false);
  expect(screen.getByRole("progressbar").hasAttribute("data-indeterminate")).toBe(true);
  expect(screen.getByRole("progressbar").querySelector<HTMLElement>(".update-track > div")?.style.width).toBe("");
  rerender(<Notices {...props} copied={null} {...{ updateProgress: { ...progress, phase: "failed" } }} />);
  expect(screen.queryByRole("progressbar")).toBeNull();
  expect(screen.getByRole("alert").textContent).toContain("Check for updates in More");
});
