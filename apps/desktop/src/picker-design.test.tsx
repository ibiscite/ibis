import { afterEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import { fixtureDestination, formatAnswer } from "./testing/format";
import { backend, chooseFromMore, openCitationDetails, prepared, receipt, resetApp, result } from "./testing/app";
import { missingKey } from "./test-fixtures";
import { inventedSource, PageSources } from "./features/reader/source";

afterEach(resetApp);

test("an editor owns the shown format and Send until its request ends", async () => {
  let editor: { request: number; format: string; style: boolean } | null = { request: 1, format: "LaTeX", style: false };
  const copied = vi.fn(() => ({ text: "\\cite{aster}", warnings: [] }));
  backend({ editor_request: () => editor, "destination": () => fixtureDestination({ format: formatAnswer("apa"), app: "Invented Writer", can_paste: true }),
    search: () => [result("aster", "Invented reference")], copy_file_citation: copied });
  render(<App />);
  await screen.findByText("Invented reference");
  expect(await screen.findByRole("button", { name: /Send.*LaTeX/ })).toBeTruthy();
  expect(screen.queryByRole("button", { name: /^Citation format/ })).toBeNull();
  await chooseFromMore("Keyboard Shortcuts");
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "F", ctrlKey: true, shiftKey: true });
  expect(screen.queryByRole("dialog", { name: "Format" })).toBeNull();
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await waitFor(() => expect(copied).toHaveBeenCalledOnce());
  editor = null;
  await act(async () => { await emit("editor:request", null); });
  expect(await screen.findByRole("button", { name: /^Citation format.*APA/ })).toBeTruthy();
});

test("a no-match search retains the prepared citation but needs fresh Enter after a queued Enter", async () => {
  let finish!: () => void;
  const unmatched = new Promise<never[]>((resolve) => { finish = () => resolve([]); });
  const preview = vi.fn(() => receipt("[@aster]"));
  const copied = vi.fn((args: unknown) => ({ text: prepared(args).preview.text, warnings: [] }));
  backend({ search: ({ query }) => query.text === "unmatched" ? unmatched : [result("aster", "Invented reference")],
    preview_file_citation: preview, copy_file_citation: copied });
  render(<App />);
  await screen.findByText("Invented reference");
  await waitFor(() => expect(screen.getByRole("region", { name: "Preview" }).textContent).toContain("[@aster]"));
  const preparedCount = preview.mock.calls.length;
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "unmatched" } });
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await act(async () => { finish(); });
  await screen.findByText("No matching references");
  expect(screen.getByRole("region", { name: "Preview" }).textContent).toContain("[@aster]");
  expect(preview).toHaveBeenCalledTimes(preparedCount);
  expect(copied).not.toHaveBeenCalled();
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await waitFor(() => expect(copied).toHaveBeenCalledOnce());
  expect(prepared(copied.mock.calls[0]![0]).request.items[0]!.id.key).toBe("aster");
});

test("citation details and the warnings Inspector explain a prepared refusal", async () => {
  const refusal = "An invented key identifies another work. Fix the key before citing.";
  backend({ search: () => [result("aster", "Invented reference")],
    preview_file_citation: () => ({ ...receipt("[@aster]", null, [missingKey("aster")]), project_refusal: refusal }) });
  render(<App />);
  await screen.findByText("Invented reference");
  await openCitationDetails();
  expect(within(screen.getByRole("dialog", { name: /^Citation details/ })).getByRole("alert").textContent).toContain(refusal);
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  fireEvent.click(screen.getByRole("button", { name: /1 warning/ }));
  expect(within(screen.getByRole("region", { name: "Citation warnings" })).getByRole("alert").textContent).toContain(refusal);
});

test("the registered Inspector shortcut opens the offered citation's details", async () => {
  backend({ search: () => [result("aster", "Invented reference")] });
  render(<App />);
  await screen.findByText("Invented reference");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "i", code: "KeyI", ctrlKey: true });
  expect(await screen.findByRole("textbox", { name: "Page" })).toBeTruthy();
});

test("the Reader keeps the refusal and the reference's warnings beside its editable fields", async () => {
  const refusal = "An invented key identifies another work. Fix the key before citing.";
  backend({ search: () => [result("aster", "Invented reference")],
    preview_file_citation: () => ({ ...receipt("[@aster]", null, [missingKey("aster")]), project_refusal: refusal }) });
  const pdf = inventedSource();
  render(<PageSources.Provider value={() => pdf}><App /></PageSources.Provider>);
  await screen.findByText("Invented reference");
  await openCitationDetails();
  fireEvent.click(screen.getByRole("button", { name: "Show PDF" }));
  const reader = screen.getByRole("region", { name: /^Citation details/ });
  expect(within(reader).getByRole("alert").textContent).toContain(refusal);
  expect(reader.querySelector(".ui-popover-note")?.textContent).toContain("aster");
});

test("header and footer name Clipboard when warnings change delivery to copy", async () => {
  backend({ "destination": () => fixtureDestination({ format: formatAnswer("bibtex"), app: "Invented Writer", can_paste: true }),
    search: () => [result("aster", "Invented reference"), result("birch", "Another invented reference")],
    preview_file_citation: ({ items }) => receipt("Invented BibTeX", null, items.length > 1 ? [missingKey("aster")] : []) });
  render(<App />);
  await screen.findByText("Invented reference");
  expect(await screen.findByRole("button", { name: /Paste into Invented Writer/ })).toBeTruthy();
  const search = screen.getByLabelText("Search references");
  fireEvent.keyDown(search, { key: "Enter", shiftKey: true });
  fireEvent.keyDown(search, { key: "ArrowDown" });
  fireEvent.keyDown(search, { key: "Enter", shiftKey: true });
  expect(await screen.findByRole("button", { name: /^Citation format.*BibTeX.*Clipboard/ })).toBeTruthy();
  const footer = screen.getByRole("region", { name: "Picker actions" });
  expect(within(footer).getByText("Clipboard")).toBeTruthy();
  expect(within(footer).getByRole("button", { name: "Copy 2 as BibTeX" })).toBeTruthy();
});
