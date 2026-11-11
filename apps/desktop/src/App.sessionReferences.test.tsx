import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import type { SessionBibliography, SessionReferencesView } from "./api";
import { backend, chooseFromMore, resetApp, result } from "./testing/app";

afterEach(resetApp);

const plain = (text: string | null | undefined) => (text ?? "").replace(/[⁨⁩]/g, "");
const taylor = result("taylor2026", "Synthetic signals"), morgan = result("morgan2025", "Synthetic noise");
const list = (over: Partial<SessionReferencesView> = {}): SessionReferencesView => ({ revision: 1, rows: [taylor, morgan], absent: 0, overflowed: false, undo: null, ...over });
const formatted = (text: string, left_out: string[] = []): SessionBibliography => ({ text, html: `<p>${text}</p>`, content_hash: `hash-${text}`, warnings: [], left_out });

async function open() {
  render(<App />);
  await chooseFromMore("Cited Since Opening…");
  return screen.findByRole("dialog");
}

test("it is named for what Ibis knows and shows what Copy writes", async () => {
  const sent: unknown[] = [];
  backend({
    session_references: () => list(), session_bibliography: () => formatted("Morgan, A. Taylor, A."),
    copy_session_bibliography: (args) => { sent.push(args); return null; },
  });
  const dialog = await open();
  expect(within(dialog).getByRole("heading", { name: "Cited Since Opening" })).toBeTruthy();
  expect(dialog.textContent).toContain("Ibis can’t see your document");
  expect(dialog.textContent).not.toMatch(/your bibliography|document's bibliography/i);
  await within(dialog).findByText("Morgan, A. Taylor, A.");
  fireEvent.click(within(dialog).getByRole("button", { name: "Copy Bibliography" }));
  await within(dialog).findByText("Copied the bibliography.");
  expect(sent).toEqual([{ style: "apa", contentHash: "hash-Morgan, A. Taylor, A." }]);
});

test("Enter copies the bibliography, as the footer says, unless a control in the sheet has the keyboard", async () => {
  const sent: unknown[] = [];
  backend({
    session_references: () => list(), session_bibliography: () => formatted("Morgan, A. Taylor, A."),
    copy_session_bibliography: (args) => { sent.push(args); return null; },
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

test("removing, undoing and resetting go through the native list and format again", async () => {
  let current = list();
  const edits: unknown[] = [];
  backend({
    session_references: () => current,
    session_bibliography: () => current.rows.length ? formatted(current.rows.map((row) => row.key).join(" ")) : null,
    edit_session_references: ({ edit }) => {
      edits.push(edit);
      const { kind } = edit as { kind: string };
      current = kind === "remove" ? list({ rows: [morgan], undo: { kind: "removed", reference: taylor } })
        : kind === "reset" ? list({ rows: [], undo: { kind: "reset" } }) : list();
      return current;
    },
  });
  const dialog = await open();
  await within(dialog).findByText("taylor2026 morgan2025");
  fireEvent.click(within(dialog).getAllByRole("button", { name: /^Remove / })[0]!);
  await within(dialog).findByText("morgan2025");
  expect(edits[0]).toEqual({ kind: "remove", id: taylor.id });
  fireEvent.click(await within(dialog).findByRole("button", { name: /^Undo Remove/ }));
  await within(dialog).findByText("taylor2026 morgan2025");
  fireEvent.click(within(dialog).getByRole("button", { name: "Reset List" }));
  await within(dialog).findByRole("button", { name: "Undo Reset List" });
  expect(plain(dialog.textContent)).toContain("Nothing yet");
  expect(within(dialog).getByRole<HTMLButtonElement>("button", { name: "Copy Bibliography" }).disabled).toBe(true);
});

test("a slow bibliography for an older list never replaces the newer one", async () => {
  const pending: ((value: SessionBibliography) => void)[] = [];
  let current = list();
  backend({
    session_references: () => current,
    session_bibliography: () => new Promise((resolve) => { pending.push(resolve); }),
    edit_session_references: () => { current = list({ rows: [morgan] }); return current; },
  });
  const dialog = await open();
  await waitFor(() => expect(pending).toHaveLength(1));
  fireEvent.click(within(dialog).getAllByRole("button", { name: /^Remove / })[0]!);
  await waitFor(() => expect(pending).toHaveLength(2));
  await act(async () => { pending[1]!(formatted("newer list")); });
  await within(dialog).findByText("newer list");
  await act(async () => { pending[0]!(formatted("older list")); });
  expect(dialog.textContent).toContain("newer list");
  expect(dialog.textContent).not.toContain("older list");
});

test("a list that moved on since it was shown is shown again, not copied", async () => {
  let calls = 0;
  backend({
    session_references: () => list(),
    session_bibliography: () => formatted(`version ${++calls}`),
    copy_session_bibliography: () => { throw { code: "snapshot_changed" }; },
  });
  const dialog = await open();
  await within(dialog).findByText("version 1");
  fireEvent.click(within(dialog).getByRole("button", { name: "Copy Bibliography" }));
  await within(dialog).findByText("version 2");
  expect(within(dialog).queryByText("Copied the bibliography.")).toBeNull();
});

test("a copy or a save that fails says so", async () => {
  backend({
    session_references: () => list(), session_bibliography: () => formatted("text"),
    copy_session_bibliography: () => { throw new Error("no sentence for a person"); },
    export_session_bibliography: () => { throw "no sentence for a person"; },
  });
  const dialog = await open();
  await within(dialog).findByText("text");
  fireEvent.click(within(dialog).getByRole("button", { name: "Copy Bibliography" }));
  await within(dialog).findByText("The bibliography could not be copied. Try again.");
  fireEvent.click(within(dialog).getByRole("button", { name: "Save…" }));
  await within(dialog).findByText("The bibliography could not be saved. Try again.");
});

test("a saved file is named, and a cancelled dialog says nothing", async () => {
  let answer: string | null = null;
  backend({ session_references: () => list(), session_bibliography: () => formatted("text"), export_session_bibliography: () => answer });
  const dialog = await open();
  await within(dialog).findByText("text");
  fireEvent.click(within(dialog).getByRole("button", { name: "Save…" }));
  await waitFor(() => expect(plain(dialog.textContent)).not.toContain("Saved"));
  answer = "Cited since opening.rtf";
  fireEvent.click(within(dialog).getByRole("button", { name: "Save…" }));
  await waitFor(() => expect(plain(dialog.textContent)).toContain("Saved Cited since opening.rtf."));
});

test("references from a library that is not open are counted, not hidden", async () => {
  backend({ session_references: () => list({ rows: [taylor], absent: 2 }), session_bibliography: () => formatted("text") });
  const dialog = await open();
  await within(dialog).findByText("2 references aren’t in the open library, so they are left out.");
});

test("a reference the style cannot write is named, and the rest can still be copied", async () => {
  backend({ session_references: () => list(), session_bibliography: () => formatted("Morgan, A.", ["taylor2026"]) });
  const dialog = await open();
  await waitFor(() => expect(plain(dialog.textContent)).toContain("Left out because this style can’t write them: taylor2026."));
  expect(within(dialog).getByRole<HTMLButtonElement>("button", { name: "Copy Bibliography" }).disabled).toBe(false);
});

test("a native refusal keeps its own words", async () => {
  backend({
    session_references: () => list(), session_bibliography: () => formatted("text"),
    export_session_bibliography: () => { throw { code: "failed", message: "Save it as .rtf, .html or .txt." }; },
  });
  const dialog = await open();
  await within(dialog).findByText("text");
  fireEvent.click(within(dialog).getByRole("button", { name: "Save…" }));
  await within(dialog).findByText("Save it as .rtf, .html or .txt.");
});

test("a removed row leaves the keyboard in the sheet, so the next key cannot act behind it", async () => {
  let current = list();
  const calls = backend({
    session_references: () => current, session_bibliography: () => formatted("text"),
    edit_session_references: () => { current = list({ rows: [morgan] }); return current; },
  });
  const dialog = await open();
  await within(dialog).findByText("text");
  const remove = within(dialog).getAllByRole("button", { name: /^Remove / })[0]!;
  act(() => remove.focus());
  fireEvent.click(remove);
  await waitFor(() => expect(within(dialog).getAllByRole("button", { name: /^Remove / })).toHaveLength(1));
  await waitFor(() => expect(dialog.contains(document.activeElement)).toBe(true));
  fireEvent.keyDown(document.activeElement!, { key: "Enter" });
  expect(calls.filter((command) => command === "copy_file_citation")).toHaveLength(0);
  expect(screen.getByRole("dialog")).toBe(dialog);
});

test("hiding the picker closes the sheet, and opening it again reads the list afresh", async () => {
  let reads = 0;
  backend({ session_references: () => { reads += 1; return list(); }, session_bibliography: () => formatted("text") });
  const dialog = await open();
  await within(dialog).findByText("text");
  await act(() => emit("picker:hidden"));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  await act(() => emit("picker:shown", 1));
  await chooseFromMore("Cited Since Opening…");
  await within(await screen.findByRole("dialog")).findByText("text");
  expect(reads).toBe(2);
});

test("opening the source with its shortcut closes the sheet instead of acting behind it", async () => {
  backend({ session_references: () => list(), session_bibliography: () => formatted("text") });
  const dialog = await open();
  await within(dialog).findByText("text");
  fireEvent.keyDown(dialog, { key: "o", ctrlKey: true });
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
});

test("a read that fails says so, and closing and reopening reads the list again", async () => {
  let fail = true, reads = 0;
  backend({
    session_references: () => { reads += 1; if (fail) throw new Error("no"); return list(); },
    session_bibliography: () => formatted("text"),
  });
  const dialog = await open();
  await within(dialog).findByText("The references could not be read. Try again.");
  fireEvent.keyDown(dialog, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  fail = false;
  await chooseFromMore("Cited Since Opening…");
  await within(await screen.findByRole("dialog")).findByText("text");
  expect(reads).toBe(2);
});
