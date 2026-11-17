import { afterEach, expect, test } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { App } from "./App";
import type { SessionBibliography, SessionReferencesView } from "./api";
import { backend, chooseFromMore, displayTree, resetApp, result } from "./testing/app";

afterEach(resetApp);

const taylor = result("taylor2026", "Synthetic signals"), morgan = result("morgan2025", "Synthetic noise");
const list = (over: Partial<SessionReferencesView> = {}): SessionReferencesView => ({ revision: 1, rows: [taylor, morgan], absent: 0, overflowed: false, undo: null, ...over });
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
