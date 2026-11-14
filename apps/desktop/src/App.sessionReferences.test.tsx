import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import type { SessionBibliography, SessionReferencesView } from "./api";
import { backend, chooseFromMore, displayTree, receipt, resetApp, result } from "./testing/app";

afterEach(resetApp);

const plain = (text: string | null | undefined) => (text ?? "").replace(/[⁨⁩]/g, "");
const taylor = result("taylor2026", "Synthetic signals"), morgan = result("morgan2025", "Synthetic noise");
const list = (over: Partial<SessionReferencesView> = {}): SessionReferencesView => ({ revision: 1, rows: [taylor, morgan], absent: 0, overflowed: false, undo: null, ...over });
const formatted = (text: string, left_out: string[] = []): SessionBibliography => ({ text, display: displayTree([{ item: 0, text }]), content_hash: `hash-${text}`, warnings: [], left_out });

async function open() {
  render(<App />);
  await chooseFromMore("Cited Since Opening…");
  return screen.findByRole("dialog");
}

test("closing slow bibliography formatting lets citation B finish before the abandoned bibliography", async () => {
  let busy = false;
  let started = false;
  backend({ search: () => [taylor, morgan], session_references: () => list(),
    session_bibliography: ({ intent }) => {
      if (intent?.cancel) { busy = false; return null; }
      started = busy = true;
      return new Promise(() => {});
    },
    preview_file_citation: ({ items }) => busy ? new Promise(() => {}) : receipt(items[0]?.id.key === morgan.id.key ? "Citation B" : "Citation A"),
  });
  const dialog = await open();
  await waitFor(() => expect(started).toBe(true));
  fireEvent.keyDown(dialog, { key: "Escape" });
  fireEvent.click(screen.getByText(morgan.title));
  expect(await screen.findByText("Citation B")).toBeTruthy();
});

test.each(["Copy Bibliography", "Save…"])("closing slow %s formatting lets citation B finish without a late delivery", async (action) => {
  let busy = false;
  let started = false;
  const deliver = ({ intent }: { intent?: { cancel: boolean } }) => {
    if (intent?.cancel) { busy = false; return null; }
    started = busy = true;
    return new Promise(() => {});
  };
  backend({ search: () => [taylor, morgan], session_references: () => list(),
    session_bibliography: () => formatted("Bibliography A"),
    copy_session_bibliography: deliver, export_session_bibliography: deliver,
    preview_file_citation: ({ items }) => busy ? new Promise(() => {}) : receipt(items[0]?.id.key === morgan.id.key ? "Citation B" : "Citation A"),
  });
  const dialog = await open();
  await within(dialog).findByText("Bibliography A");
  fireEvent.click(within(dialog).getByRole("button", { name: action }));
  await waitFor(() => expect(started).toBe(true));
  fireEvent.keyDown(dialog, { key: "Escape" });
  fireEvent.click(screen.getByText(morgan.title));
  expect(await screen.findByText("Citation B")).toBeTruthy();
});

test("it is named for what Ibis knows and shows what Copy writes", async () => {
  const sent: unknown[] = [];
  backend({
    session_references: () => list(), session_bibliography: () => formatted("Morgan, A. Taylor, A."),
    copy_session_bibliography: ({ style, contentHash }) => { sent.push({ style, contentHash }); return null; },
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
    session_bibliography: ({ intent }) => intent?.cancel ? null : new Promise((resolve) => { pending.push(resolve); }),
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
  expect(within(dialog).getByRole("alert").textContent).toMatch(/list changed/i);
});

test.each(["refused", "already copied"] as const)("resetting during a delayed copy keeps the empty list and retires %s feedback", async (outcome) => {
  let current = list();
  const clipboard = { text: "Invented clipboard text" };
  let finish: () => void = () => { throw new Error("Copy has not started"); };
  backend({
    session_references: () => current,
    session_bibliography: () => current.rows.length ? formatted("Taylor and Morgan") : null,
    edit_session_references: () => { current = list({ revision: 2, rows: [], undo: { kind: "reset" } }); return current; },
    copy_session_bibliography: () => {
      const revision = current.revision;
      if (outcome === "already copied") clipboard.text = "Taylor and Morgan";
      return new Promise<void>((resolve, reject) => {
        finish = () => {
          if (outcome === "refused" && revision !== current.revision) { reject({ code: "snapshot_changed" }); return; }
          clipboard.text = "Taylor and Morgan";
          resolve();
        };
      });
    },
  });
  const dialog = await open();
  await within(dialog).findByText("Taylor and Morgan");
  fireEvent.click(within(dialog).getByRole("button", { name: "Copy Bibliography" }));
  const beforeReset = clipboard.text;
  fireEvent.click(within(dialog).getByRole("button", { name: "Reset List" }));
  await within(dialog).findByRole("button", { name: "Undo Reset List" });
  await act(async () => { finish(); });

  expect(plain(dialog.textContent)).toContain("Nothing yet");
  expect(current.rows).toEqual([]);
  expect(clipboard.text).toBe(beforeReset);
  expect(within(dialog).getByRole<HTMLButtonElement>("button", { name: "Copy Bibliography" }).disabled).toBe(true);
  expect(within(dialog).queryByText("Copied the bibliography.")).toBeNull();
  expect(within(dialog).getByRole("alert").textContent).toMatch(/list changed/i);
});

test("a delayed copy cannot replace the failure from a newer list edit", async () => {
  let finish: () => void = () => { throw new Error("Copy has not started"); };
  backend({
    session_references: () => list(), session_bibliography: () => formatted("Taylor and Morgan"),
    copy_session_bibliography: () => new Promise<void>((resolve) => { finish = resolve; }),
    edit_session_references: () => { throw new Error("Invented list failure"); },
  });
  const dialog = await open();
  await within(dialog).findByText("Taylor and Morgan");
  fireEvent.click(within(dialog).getByRole("button", { name: "Copy Bibliography" }));
  fireEvent.click(within(dialog).getByRole("button", { name: "Reset List" }));
  await within(dialog).findByText("The references could not be read. Try again.");
  await act(async () => { finish(); });
  expect(within(dialog).getByRole("alert").textContent).toBe("The references could not be read. Try again.");
  expect(within(dialog).queryByText("Copied the bibliography.")).toBeNull();
});

test.each([
  ["Copy Bibliography", "copy_session_bibliography", "The bibliography could not be copied. Try again.", "Copied the bibliography."],
  ["Save…", "export_session_bibliography", "Save it as .rtf, .html or .txt.", "Saved invented.rtf."],
])("Try Again repeats %s after delivery fails", async (action, command, failure, success) => {
  let attempts = 0;
  const calls = backend({
    session_references: () => list(), session_bibliography: () => formatted("text"),
    [command]: () => {
      if (++attempts === 1) throw command === "copy_session_bibliography" ? new Error("invented failure")
        : { code: "bibliography-format", message: { id: "app-bibliography-format", args: {} }, recovery: { action: "retry" }, focus: "recovery-action" };
      return "invented.rtf";
    },
  });
  const dialog = await open();
  await within(dialog).findByText("text");
  fireEvent.click(within(dialog).getByRole("button", { name: action }));
  await within(dialog).findByText(failure);
  fireEvent.click(within(dialog).getByRole("button", { name: "Try Again" }));
  await waitFor(() => expect(plain(dialog.textContent)).toContain(success));
  expect(attempts).toBe(2);
  expect(calls.filter((name) => name === "session_references")).toHaveLength(1);
  expect(calls.filter((name) => name === "session_bibliography")).toHaveLength(1);
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

test.each(["read", "format"])("Try Again reloads after a %s failure", async (operation) => {
  let fail = true, reads = 0;
  backend({
    session_references: () => { reads += 1; if (fail && operation === "read") throw new Error("invented failure"); return list(); },
    session_bibliography: () => { if (fail && operation === "format") throw new Error("invented failure"); return formatted("text"); },
  });
  const dialog = await open();
  await within(dialog).findByRole("alert");
  fail = false;
  fireEvent.click(within(dialog).getByRole("button", { name: "Try Again" }));
  await within(dialog).findByText("text");
  expect(reads).toBe(2);
});

test("an all-refused bibliography offers the sheet’s Remove controls and removal retries it", async () => {
  let current = list({ rows: [taylor] });
  const calls = backend({ session_references: () => current,
    session_bibliography: () => {
      if (current.rows.length === 0) return null;
      throw { code: "bibliography-unsupported", message: { id: "app-command-bibliography-unsupported", args: { references: "taylor2026" } },
        recovery: { action: "remove-references" }, focus: "session-references" };
    },
    edit_session_references: () => { current = list({ rows: [] }); return current; },
  });
  const dialog = await open();
  expect((await within(dialog).findByRole("alert")).textContent).toContain("Remove them from this list to continue.");
  expect(within(dialog).getByRole("button", { name: "Copy Bibliography" }).hasAttribute("disabled")).toBe(true);
  fireEvent.click(within(dialog).getByRole("button", { name: /^Remove / }));
  await waitFor(() => expect(within(dialog).queryByRole("alert")).toBeNull());
  expect(calls.filter((command) => command === "session_bibliography")).toHaveLength(2);
});
