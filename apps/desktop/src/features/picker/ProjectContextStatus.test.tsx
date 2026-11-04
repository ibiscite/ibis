import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ProjectCheckStatus, WritingProjectTaskStatus } from "./ProjectContextStatus";

afterEach(cleanup);

const active = { id: "thesis", name: "Thesis", output: "autocite", bibliography: "thesis.bib", revision: "1",
  pending: { name: "Thesis", output: "pandoc", bibliography: "paper.bib", revision: "2" } };

test("pending updates are a persistent nonmodal action beside the task's current choices", () => {
  const apply = vi.fn();
  render(<><button>Insert citation</button><WritingProjectTaskStatus project={active} onApply={apply} /></>);
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.getByRole<HTMLButtonElement>("button", { name: "Insert citation" }).disabled).toBe(false);
  expect(screen.getByRole("status").textContent).toContain("thesis.bib");
  expect(screen.getByRole("status").textContent).not.toContain("paper.bib");
  fireEvent.click(screen.getByRole("button", { name: "Apply Updates" }));
  expect(apply).toHaveBeenCalledOnce();
});

test("retained settings are named and offer no update from a deleted definition", () => {
  render(<WritingProjectTaskStatus project={{ ...active, pending: null, retained: true }} onApply={vi.fn()} />);
  expect(screen.getByRole("status").textContent).toMatch(/retained settings/i);
  expect(screen.getByRole("status").textContent).toContain("thesis.bib");
  expect(screen.queryByRole("button", { name: "Apply Updates" })).toBeNull();
});

test.each([false, true])("a paused check offers a deliberate one-off %s action plus recovery", (paste) => {
  const retry = vi.fn(), locate = vi.fn(), unverified = vi.fn();
  render(<ProjectCheckStatus check="paused" message="Couldn’t check this citation against thesis.bib." paste={paste}
    onRetry={retry} onLocate={locate} onUnverified={unverified} />);
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  fireEvent.click(screen.getByRole("button", { name: "Locate Bibliography…" }));
  fireEvent.click(screen.getByRole("button", { name: paste ? "Paste Unverified Key" : "Copy Unverified Key" }));
  expect(retry).toHaveBeenCalledOnce();
  expect(locate).toHaveBeenCalledOnce();
  expect(unverified).toHaveBeenCalledOnce();
  expect(screen.queryByRole("dialog")).toBeNull();
});

test("known conflicts cannot offer an unchecked override even if a caller supplies one", () => {
  render(<ProjectCheckStatus check="conflict" message="This key identifies another work." onRetry={vi.fn()} onUnverified={vi.fn()} />);
  expect(screen.getByRole("alert").textContent).toContain("another work");
  expect(screen.queryByRole("button", { name: /Unverified Key/ })).toBeNull();
});

test("retained evidence shows its localized check time and Retry without an unverified override", () => {
  const checkedAtMs = Date.UTC(2026, 10, 4, 15, 42);
  const time = new Intl.DateTimeFormat(undefined, { dateStyle: "short", timeStyle: "short" }).format(new Date(checkedAtMs));
  const retry = vi.fn();
  render(<ProjectCheckStatus check="retained" message="Couldn’t read thesis.bib. Using its last checked copy." checkedAtMs={checkedAtMs}
    onRetry={retry} onUnverified={vi.fn()} />);
  expect(screen.getByRole("status").textContent).toContain(time);
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  expect(retry).toHaveBeenCalledOnce();
  expect(screen.queryByRole("button", { name: /Unverified Key/ })).toBeNull();
});

test("retained evidence never fabricates an unavailable check time", () => {
  render(<ProjectCheckStatus check="retained" message="Using the last checked copy." checkedAtMs={Number.NaN} />);
  expect(screen.getByRole("status").textContent).toContain("The last checked time is unavailable.");
});

test("changing between bibliography files with the same name is still stated without exposing their locations", () => {
  render(<WritingProjectTaskStatus project={{ ...active, pending: { ...active.pending, bibliography: "thesis.bib", bibliography_changed: true } }} onApply={vi.fn()} />);
  expect(screen.getByRole("status").textContent).toContain("The bibliography file or location changed.");
  expect(screen.getByRole("status").textContent).toContain("thesis.bib");
});
