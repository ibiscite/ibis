import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { ProposalResult, WaitingProposal } from "../bindings";
import { deferred } from "../testing/deferred";
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

test("an engine refusal appears verbatim and keeps Reject available", async () => {
  const sentence = "The reference changed since this was proposed; nothing was written.";
  page([bib], { applyProposal: vi.fn(async () => ({ status: "refused", sentence })) });
  fireEvent.click(await screen.findByRole("button", { name: /^Apply/ }));
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", sentence);
  expect(screen.getByRole("button", { name: /^Reject (?!all)/ })).toBeTruthy();
});

test("Reject dismisses its own proposal without applying", async () => {
  const { api } = page();
  fireEvent.click(await screen.findByRole("button", { name: "Reject Claude Desktop · signals.bib" }));
  await act(async () => {});
  expect(api.rejectProposal).toHaveBeenCalledExactlyOnceWith(bib.id);
  expect(screen.queryByText("Claude Desktop · signals.bib")).toBeNull();
  expect(screen.getByText("Cursor · signals.bib")).toBeTruthy();
  expect(api.applyProposal).not.toHaveBeenCalled();
});

test("the empty group has one row", async () => {
  page([]);
  const empty = await screen.findByText("No proposed changes");
  expect(empty.closest(".ui-group")?.querySelectorAll(".ui-row")).toHaveLength(1);
  expect(screen.queryByRole("button")).toBeNull();
});

test.each(["apply", "reject", "reject-app"])("a late list cannot overwrite a newer %s", async (action) => {
  const late = deferred<WaitingProposal[]>();
  page([bib], { waitingProposals: vi.fn().mockResolvedValueOnce([bib]).mockReturnValueOnce(late.promise) });
  const button = await screen.findByRole("button", { name: action === "apply" ? /^Apply/ : action === "reject" ? /^Reject (?!all)/ : /^Reject all/ });
  fireEvent(window, new Event("focus"));
  fireEvent.click(button);
  await act(async () => {});
  await act(async () => late.resolve([bib]));
  if (action === "apply") expect(screen.getByRole("status").textContent).toContain("Applied;");
  else expect(screen.getByText("No proposed changes")).toBeTruthy();
  expect(screen.queryByRole("button", { name: /^Apply/ })).toBeNull();
});

test("hidden windows skip reads, becoming visible refreshes, and newer reads win", async () => {
  const late = deferred<WaitingProposal[]>();
  const { api } = page([bib]);
  await screen.findByText("Claude Desktop · signals.bib");
  vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
  fireEvent(window, new Event("focus"));
  fireEvent(document, new Event("visibilitychange"));
  expect(api.waitingProposals).toHaveBeenCalledTimes(1);
  vi.restoreAllMocks();
  api.waitingProposals.mockReturnValueOnce(late.promise).mockResolvedValueOnce([]);
  fireEvent(document, new Event("visibilitychange"));
  fireEvent(window, new Event("focus"));
  await screen.findByText("No proposed changes");
  await act(async () => late.resolve([bib]));
  expect(screen.getByText("No proposed changes")).toBeTruthy();
});

test("focus during a decision waits for its result and cannot erase the receipt", async () => {
  const apply = deferred<ProposalResult>();
  const { api } = page([bib], { applyProposal: vi.fn(() => apply.promise) });
  fireEvent.click(await screen.findByRole("button", { name: /^Apply/ }));
  api.waitingProposals.mockResolvedValueOnce([]);
  fireEvent(window, new Event("focus"));
  expect(api.waitingProposals).toHaveBeenCalledTimes(1);
  await act(async () => apply.resolve({ status: "applied", backup: "signals.bib.ibis-backup-1" }));
  expect(api.waitingProposals).toHaveBeenCalledTimes(2);
  expect(screen.getByRole("status").textContent).toContain("signals.bib.ibis-backup-1");
});

test("a refused rejection keeps the row and shows the engine sentence", async () => {
  page([bib], { rejectProposal: vi.fn().mockResolvedValue({ status: "refused", sentence: "This proposal was applied or rejected already." }) });
  fireEvent.click(await screen.findByRole("button", { name: /^Reject (?!all)/ }));
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", "This proposal was applied or rejected already.");
  expect(screen.getByRole("button", { name: /^Reject (?!all)/ })).toHaveProperty("disabled", false);
});


test("Reject all from this app removes its pending rows across libraries and keeps other apps and Apply receipts", async () => {
  const { api } = page([bib, { ...bib, id: "same-app-other-library", library: "other.bib" }, exampleProposals()[1]!]);
  fireEvent.click(await screen.findByRole("button", { name: "Apply Claude Desktop · signals.bib" }));
  await screen.findByRole("status");
  fireEvent.click(screen.getByRole("button", { name: "Reject all from this app Claude Desktop · other.bib" }));
  await act(async () => {});
  expect(api.rejectAppProposals).toHaveBeenCalledExactlyOnceWith("same-app-other-library");
  expect(api.rejectProposal).not.toHaveBeenCalled();
  expect(screen.queryByText("Claude Desktop · other.bib")).toBeNull();
  expect(screen.getByText("Cursor · signals.bib")).toBeTruthy();
  expect(screen.getByRole("status").textContent).toContain("Applied;");
});

test("a failed app rejection keeps proposals and shows its refusal", async () => {
  page([bib], { rejectAppProposals: vi.fn().mockResolvedValue({ status: "refused", sentence: "Invented refusal" }) });
  fireEvent.click(await screen.findByRole("button", { name: /^Reject all/ }));
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", "Invented refusal");
  expect(screen.getByRole("button", { name: /^Apply/ })).toBeTruthy();
});
