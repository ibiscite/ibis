import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { ProposalResult, WaitingProposal } from "../bindings";
import { exampleProposals } from "../testing/proposals";
import { ProposedChanges } from "./ProposedChanges";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

const bib = exampleProposals()[0]!;

function page(proposals: WaitingProposal[] = exampleProposals(), overrides = {}) {
  const api = {
    waitingProposals: vi.fn(async () => proposals),
    applyProposal: vi.fn(async (_id: string): Promise<ProposalResult> => ({ status: "applied", backup: "signals.bib.ibis-backup-1" })),
    rejectAppProposals: vi.fn(async (_id: string): Promise<ProposalResult> => ({ status: "rejected" })),
    rejectProposal: vi.fn(async (_id: string): Promise<ProposalResult> => ({ status: "rejected" })),
    ...overrides,
  };
  return { api, ...render(<ProposedChanges api={api} highlighted={null} />) };
}

test("shows the source, target, summary and engine before/after without applying", async () => {
  const { api } = page();
  expect(await screen.findByText("Claude Desktop · signals.bib")).toBeTruthy();
  expect(screen.getByText("Cursor · signals.bib")).toBeTruthy();
  expect(screen.getByText(/Change title of “River signals”/)).toBeTruthy();
  const code = screen.getByText("Claude Desktop · signals.bib").closest(".ui-row")!.querySelector(".ui-note code");
  expect(code?.textContent).toContain("-   title = {River signals},");
  expect(code?.textContent).toContain("+   title = {River signals revisited},");
  for (const label of ["Claude Desktop · signals.bib", "Cursor · signals.bib"]) {
    const row = screen.getByText(label).closest(".ui-row")! as HTMLElement;
    expect(within(row).getAllByRole("button").map((button) => button.textContent)).toEqual(["Apply", "Reject"]);
  }
  expect(screen.queryByRole("button", { name: /^Reject All/ })).toBeNull();
  expect(api.applyProposal).not.toHaveBeenCalled();
});

test("Apply sends only the displayed id and retains its backup receipt on focus", async () => {
  const { api } = page([bib]);
  fireEvent.click(await screen.findByRole("button", { name: "Apply Claude Desktop · signals.bib" }));
  expect(await screen.findByRole("status")).toHaveProperty("textContent", "Applied; backup kept as signals.bib.ibis-backup-1.");
  expect(api.applyProposal).toHaveBeenCalledExactlyOnceWith(bib.id);
  expect(screen.queryByRole("button", { name: /^Apply/ })).toBeNull();
  api.waitingProposals.mockResolvedValueOnce([]);
  fireEvent(window, new Event("focus"));
  await act(async () => {});
  expect(screen.getByRole("status").textContent).toContain("Applied; backup kept as");
});

test("an engine refusal appears in the catalogue's words and keeps Reject available", async () => {
  const problem = { code: "changed", message: { id: "refusal-changed", args: {} }, recovery: { action: "edit-input" }, focus: "request" } as const;
  page([bib], { applyProposal: vi.fn(async (): Promise<ProposalResult> => ({ status: "refused", problem })) });
  fireEvent.click(await screen.findByRole("button", { name: /^Apply/ }));
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", "The reference changed since this was proposed; nothing was written.");
  expect(screen.getByRole("button", { name: /^Reject (?!all)/ })).toBeTruthy();
});
