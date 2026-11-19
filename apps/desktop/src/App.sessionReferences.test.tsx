import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "./App";
import type { SessionBibliography, SessionReferencesView } from "./api";
import { backend, chooseFromMore, displayTree, lineReceipt, prepared, resetApp, result } from "./testing/app";
import { fixtureDestination, formatAnswer } from "./testing/format";

afterEach(resetApp);

const taylor = result("taylor2026", "Synthetic signals"), morgan = result("morgan2025", "Synthetic noise");
const list = (over: Partial<SessionReferencesView> = {}): SessionReferencesView => ({ revision: 1, rows: [taylor, morgan], absent: 0, overflowed: false, undo: null, here: [], ...over });
const formatted = (text: string, left_out: string[] = []): SessionBibliography => ({ text, display: displayTree([{ item: 0, text }]), content_hash: `hash-${text}`, warnings: [], left_out });

async function open() {
  render(<App />);
  await chooseFromMore("Cited Since Opening…");
  return screen.findByRole("dialog");
}

test("Enter copies the bibliography, as the footer says, unless a control in the sheet has the keyboard", async () => {
  const sent: unknown[] = [];
  backend({
    session_references: () => list(), session_bibliography: () => formatted("Morgan, A. Taylor, A."),
    copy_session_bibliography: ({ style, contentHash }) => { sent.push({ style, contentHash }); return null; },
  });
  const dialog = await open();
  await within(dialog).findByText("Morgan, A. Taylor, A.");
  const remove = within(dialog).getAllByRole("button", { name: /^Remove/ })[0]!;
  fireEvent.keyDown(remove, { key: "Enter" });
  expect(sent).toEqual([]);
  fireEvent.keyDown(dialog, { key: "Enter" });
  await within(dialog).findByText("Copied the bibliography.");
  expect(sent).toEqual([{ style: "apa", contentHash: "hash-Morgan, A. Taylor, A." }]);
});

test("an app with citations from this run offers its reference list, pasted as its entries", async () => {
  const delivered: { forms: unknown[]; keys: unknown[] }[] = [];
  const here = (cited: number) => fixtureDestination({ app: "Example Writer", app_id: "example.writer", can_paste: true, cited_here: cited, format: formatAnswer("apa", "app") });
  let cited = 0;
  backend({
    search: ({ query }) => query.text ? [] : [{ ...taylor, recent: "cited" }, { ...morgan, recent: "cited" }],
    destination: () => here(cited),
    session_references: () => list({ here: [morgan, taylor] }),
    preview_file_citation: ({ items }) => lineReceipt((items as { id: unknown }[]).map(() => "Entry.")),
    deliver_citation: (args) => {
      const { request } = prepared(args);
      delivered.push({ forms: request.items.map((item) => item.form), keys: request.items.map((item) => item.id.key) });
      return { outcome: "retained", copied: { text: "Entry.", warnings: [] }, reason: "pinned" };
    },
  });
  // Nothing cited into this app yet: neither the row nor More offers it.
  const { unmount } = render(<App />);
  await screen.findByText("Synthetic signals");
  expect(screen.queryByRole("option", { name: /Paste Reference List/ })).toBeNull();
  unmount();
  cited = 2;
  render(<App />);
  const row = await screen.findByRole("option", { name: /Paste Reference List/ });
  expect(row.textContent).toContain("The 2 references you cited in Example Writer since Ibis opened");
  fireEvent.click(row);
  await waitFor(() => expect(delivered).toEqual([{ forms: ["bibliography-only", "bibliography-only"], keys: [morgan.id.key, taylor.id.key] }]));
  // More offers it too, also over a typed search, which it clears.
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "zzz" } });
  await chooseFromMore("Paste Reference List");
  await waitFor(() => expect(delivered).toHaveLength(2));
  expect(delivered[1]?.forms).toEqual(["bibliography-only", "bibliography-only"]);
  expect((screen.getByLabelText("Search references") as HTMLInputElement).value).toBe("");
});

test("a delayed reference list cannot replace a newer search or deliver its entries", async () => {
  let finishList: (value: SessionReferencesView) => void = () => {};
  const calls = backend({
    search: ({ query }) => query.text ? [morgan] : [{ ...taylor, recent: "cited" }],
    destination: () => fixtureDestination({ app: "Example Writer", app_id: "example.writer", can_paste: true, cited_here: 1, format: formatAnswer("apa", "app") }),
    session_references: () => new Promise<SessionReferencesView>((resolve) => { finishList = resolve; }),
  });
  render(<App />);
  fireEvent.click(await screen.findByRole("option", { name: /Paste Reference List/ }));
  expect(calls).toContain("session_references");
  const search = screen.getByLabelText<HTMLInputElement>("Search references");
  fireEvent.change(search, { target: { value: "noise" } });
  await screen.findByText("Synthetic noise");
  await act(async () => { finishList(list({ here: [taylor] })); });
  expect(search.value).toBe("noise");
  expect(calls).not.toContain("deliver_citation");
});
