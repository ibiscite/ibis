import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Notices } from "./Notices";

afterEach(cleanup);
const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
const noticeStyles = readFileSync(new FileURL("./Notices.css", import.meta.url), "utf8");
const copied = { text: "[@example2026]", warnings: ["Missing year — 研究 العربية <literal>"] };
const props = { library: null, failures: {}, error: null, copyError: null,
  copied, confirmation: copied, onDismissCopied: vi.fn() };

test("zoom uses the existing copied feedback position and leaves warnings visible", () => {
  const { rerender } = render(<Notices {...props} feedback="110%" />);
  expect(screen.getByText("110%")).toBeTruthy();
  expect(screen.queryByText("Copied")).toBeNull();
  expect(screen.getByLabelText("Copy warnings").textContent).toBe(copied.warnings[0]);
  rerender(<Notices {...props} />);
  expect(screen.getByText("Copied")).toBeTruthy();
});

test("a long citation refusal is a complete keyboard-reachable notice before Details opens", () => {
  const reason = "Reference 1: Choose another form or clear Page. ".repeat(30);
  const style = document.createElement("style"); style.textContent = noticeStyles; document.head.append(style);
  try {
    render(<div className="picker-content"><Notices {...props} copied={null} citationProblem={reason} /></div>);
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toBe(reason);
    expect(alert.tabIndex).toBe(0);
    expect(alert.classList.contains("citation-problem")).toBe(true);
    const appearance = getComputedStyle(alert);
    expect(appearance.whiteSpace).toBe("normal");
    expect(appearance.overflow).toBe("visible");
    expect(appearance.overflowWrap).toBe("anywhere");
    expect(appearance.maxBlockSize).toBe("none");
    expect(appearance.inlineSize).toBe("100%");
    expect(appearance.boxSizing).toBe("border-box");
    expect(getComputedStyle(alert.parentElement!).inlineSize).toBe("100%");
    expect(getComputedStyle(alert.closest(".picker-content")!).scrollBehavior).toBe("auto");
    alert.focus(); expect(document.activeElement).toBe(alert);
    expect(screen.queryByRole("dialog")).toBeNull();
  } finally { style.remove(); }
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

test("retiring a confirmation leaves its warnings without announcing Copied again", () => {
  const { rerender } = render(<Notices {...props} />);
  expect(screen.getByRole("status", { name: "" }).textContent).toContain("Copied");
  rerender(<Notices {...props} confirmation={null} />);
  expect(screen.queryByRole("status", { name: "" })).toBeNull();
  expect(screen.getByLabelText("Copy warnings").textContent).toBe(copied.warnings[0]);
  expect(screen.queryByText("Copied")).toBeNull();
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
  expect(screen.getByRole("alert").textContent).toContain("Check now in Settings > About");
});
