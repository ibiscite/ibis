import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import type { PickerDestination, WritingProjectsView } from "./api";
import type { FilePreview } from "./bindings";
import { backend, prepared, receipt, resetApp, result } from "./testing/app";

afterEach(resetApp);

const pending = <T,>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
};
const writingDestination = (name: string, format: "cite" | "typst", context: [number, number]): PickerDestination => ({
  app: "Example Editor", app_id: "example.editor", app_identity: null, automatic: null, remembered: "pandoc",
  unreadable: false, can_paste: false, output_question: "editor", paste: null, paste_question: false, can_choose_paste: false,
  writing_project: { id: name, name, output: format, bibliography: "refs.bib" },
  writing_context: `opening-${context[0]}`, context_version: context,
});

test("Enter pressed during reopening waits for the newly captured project and copies its fresh preview once", async () => {
  const first = writingDestination("Project A", "cite", [1, 0]);
  const second = writingDestination("Project B", "typst", [2, 0]);
  const reopening = pending<PickerDestination>(), settled = pending<PickerDestination>(), preview = pending<FilePreview>();
  const copied: string[] = [];
  let reads = 0, initialPreviewStarted = false, newPreviewStarted = false;
  backend({
    search: () => [result("a", "Synthetic source")],
    picker_destination: () => ++reads === 1 ? first : reads === 2 ? reopening.promise : settled.promise,
    preview_file_citation: (request) => {
      if (request.output === "typst" && request.intent.context?.[0] === 2) { newPreviewStarted = true; return preview.promise; }
      if (request.output === "cite" && request.intent.context?.[0] === 1) initialPreviewStarted = true;
      return receipt(`${request.output}:${JSON.stringify(request.intent.context)}`);
    },
    copy_file_citation: (args) => { const text = prepared(args).preview.text; copied.push(text); return { text, warnings: [] }; },
  });
  render(<App />);
  await screen.findByText("Synthetic source");
  await waitFor(() => expect(initialPreviewStarted).toBe(true));
  await act(async () => { await emit("picker:hidden"); await emit("picker:preparing", null); await emit("picker:shown", 2); });
  await waitFor(() => expect(reads).toBe(2));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  expect(copied).toEqual([]);
  await act(async () => { await emit("project:changed", [2, 0]); });
  await waitFor(() => expect(reads).toBe(3));
  await act(async () => { settled.resolve(second); });
  await waitFor(() => expect(newPreviewStarted).toBe(true));
  expect(copied).toEqual([]);
  await act(async () => { preview.resolve(receipt("typst:[2,0]")); });
  await waitFor(() => expect(copied).toEqual(["typst:[2,0]"]));
  // The slower destination worker belongs to the earlier read, even though it
  // completes after the new project's receipt has already been copied.
  await act(async () => { reopening.resolve(first); });
  expect(copied).toEqual(["typst:[2,0]"]);
});

test("a bibliography read finishing for the same project refreshes a waiting Enter without cancelling it", async () => {
  let destination = writingDestination("Project A", "cite", [1, 0]);
  const earlier = pending<FilePreview>(), checked = pending<FilePreview>();
  const copied: string[] = [];
  let delayedPreviewStarted = false, checkedPreviewStarted = false;
  backend({
    search: () => [result("a", "Synthetic source")], picker_destination: () => destination,
    preview_file_citation: (request) => {
      if (request.intent.context?.[1] === 1) { checkedPreviewStarted = true; return checked.promise; }
      if (request.intent.context?.[0] === 1) { delayedPreviewStarted = true; return earlier.promise; }
      return receipt("initial context");
    },
    copy_file_citation: (args) => { const text = prepared(args).preview.text; copied.push(text); return { text, warnings: [] }; },
  });
  render(<App />);
  await screen.findByText("Synthetic source");
  await waitFor(() => expect(delayedPreviewStarted).toBe(true));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  expect(copied).toEqual([]);
  destination = writingDestination("Project A", "cite", [1, 1]);
  await act(async () => { await emit("project:changed", [1, 1]); });
  await waitFor(() => expect(checkedPreviewStarted).toBe(true));
  await act(async () => { checked.resolve(receipt("cite:[1,1] checked against refs.bib")); });
  await waitFor(() => expect(copied).toEqual(["cite:[1,1] checked against refs.bib"]));
  await act(async () => { earlier.resolve(receipt("cite:[1,0] before bibliography read")); });
  expect(copied).toEqual(["cite:[1,1] checked against refs.bib"]);
});

test("leaving a writing project restores the app's output and copies a fresh preview", async () => {
  const project = { id: "synthetic-project", name: "Field Guide", output: "cite", bibliography: null };
  let generation = 1;
  let selected: string | null = project.id;
  const view = (): WritingProjectsView => ({ projects: [project], selected, context: `opening-${generation}`, session_only: false });
  const destination = (): PickerDestination => ({ app: "Example Editor", app_id: "example.editor", app_identity: null,
    automatic: null, remembered: "typst", unreadable: false, can_paste: false, output_question: "editor",
    paste: null, paste_question: false, can_choose_paste: false, writing_project: selected ? project : null,
    writing_context: `opening-${generation}`, context_version: [generation, 0] });
  const copied: string[] = [];
  backend({
    search: () => [result("a", "Synthetic source")], picker_destination: destination, writing_projects: view,
    edit_writing_project: async ({ context, action }) => {
      expect(context).toBe(`opening-${generation}`);
      expect(action).toEqual({ kind: "select", id: null });
      selected = null;
      generation += 1;
      await emit("project:changed", [generation, 0]);
      return view();
    },
    preview_file_citation: (request) => receipt(`${request.output}:${JSON.stringify(request.intent.context)}`),
    copy_file_citation: (args) => { const text = prepared(args).preview.text; copied.push(text); return { text, warnings: [] }; },
  });
  render(<App />);
  await screen.findByText("Synthetic source");
  const formatButton = () => within(screen.getByLabelText("Search references").closest("header")!).getByRole("button", { name: /^Citation format / });
  await waitFor(() => expect(formatButton().getAttribute("aria-label")).toContain("Field Guide"));
  fireEvent.click(formatButton());
  fireEvent.click(await screen.findByRole("menuitem", { name: "Writing Projects…" }));
  const dialog = await screen.findByRole("dialog", { name: "Writing Projects" });
  fireEvent.click(await within(dialog).findByRole("button", { name: "No Writing Project" }));
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "Writing Projects" })).toBeNull());
  await waitFor(() => expect(formatButton().getAttribute("aria-label")).toContain("Typst"));
  expect(formatButton().getAttribute("aria-label")).not.toContain("Field Guide");
  await act(async () => { fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" }); });
  await waitFor(() => expect(copied).toEqual(["typst:[2,0]"]));
});

async function openWritingProjects() {
  const format = within(screen.getByLabelText("Search references").closest("header")!).getByRole<HTMLButtonElement>("button", { name: /^Citation format / });
  await waitFor(() => expect(format.disabled).toBe(false));
  fireEvent.click(format);
  fireEvent.click(await screen.findByRole("menuitem", { name: "Writing Projects…" }));
  return screen.findByRole("dialog", { name: "Writing Projects" });
}

test("a fresh Enter after selecting a project waits for its delayed destination and copies the new format once", async () => {
  const first = writingDestination("Project A", "cite", [1, 0]);
  const second = writingDestination("Project B", "typst", [2, 0]);
  const projects = [first.writing_project!, second.writing_project!];
  const save = pending<WritingProjectsView>(), destinationRead = pending<PickerDestination>();
  let saved = false, destinationReadStarted = false;
  const copied: string[] = [];
  const view = (): WritingProjectsView => ({ projects, selected: saved ? "Project B" : "Project A", context: saved ? "opening-2" : "opening-1", session_only: false });
  backend({
    search: () => [result("a", "Synthetic source")],
    picker_destination: () => { if (!saved) return first; destinationReadStarted = true; return destinationRead.promise; },
    writing_projects: view,
    edit_writing_project: async ({ context, action }) => {
      expect(context).toBe("opening-1");
      expect(action).toEqual({ kind: "select", id: "Project B" });
      const next = await save.promise;
      saved = true;
      await emit("project:changed", [2, 0]);
      return next;
    },
    preview_file_citation: (request) => receipt(`${request.output}:${JSON.stringify(request.intent.context)}`),
    copy_file_citation: (args) => { const text = prepared(args).preview.text; copied.push(text); return { text, warnings: [] }; },
  });
  render(<App />);
  await screen.findByText("Synthetic source");
  const dialog = await openWritingProjects();
  fireEvent.click(await within(dialog).findByRole("button", { name: "Use Project B" }));
  expect(screen.getByRole("dialog", { name: "Writing Projects" })).toBeTruthy();
  await act(async () => { save.resolve({ ...view(), selected: "Project B", context: "opening-2" }); });
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "Writing Projects" })).toBeNull());
  await waitFor(() => expect(destinationReadStarted).toBe(true));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  expect(copied).toEqual([]);
  await act(async () => { destinationRead.resolve(second); });
  await waitFor(() => expect(copied).toEqual(["typst:[2,0]"]));
});

test("a same-format project switch starts a new copy and an older copy cannot deliver its proof or replace the receipt", async () => {
  const first = writingDestination("Project A", "cite", [1, 0]);
  const second = writingDestination("Project B", "cite", [2, 0]);
  const projects = [first.writing_project!, second.writing_project!];
  const oldCopy = pending<{ text: string; warnings: string[]; delivery: number }>();
  let selected = "Project A";
  const invoked: string[] = [], delivered: { text: unknown; delivery: unknown }[] = [];
  let hidden = 0;
  const view = (): WritingProjectsView => ({ projects, selected, context: selected === "Project A" ? "opening-1" : "opening-2", session_only: false });
  backend({
    search: () => [result("a", "Synthetic source")],
    picker_destination: () => selected === "Project A" ? first : second,
    writing_projects: view,
    edit_writing_project: async () => { selected = "Project B"; await emit("project:changed", [2, 0]); return view(); },
    preview_file_citation: (request) => receipt(request.intent.context?.[0] === 2 ? "cite:Project B" : "cite:Project A"),
    copy_file_citation: (args) => {
      const text = prepared(args).preview.text;
      invoked.push(text);
      return text === "cite:Project A" ? oldCopy.promise : { text, warnings: [], delivery: 29 };
    },
    hide_picker: () => { hidden += 1; return true; },
    show_copied_notice: ({ text, delivery }) => { delivered.push({ text, delivery }); return null; },
  });
  render(<App />);
  await screen.findByText("Synthetic source");
  await waitFor(() => expect(document.querySelector(".shelf")?.textContent).toContain("cite:Project A"));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await waitFor(() => expect(invoked).toEqual(["cite:Project A"]));
  const dialog = await openWritingProjects();
  fireEvent.click(await within(dialog).findByRole("button", { name: "Use Project B" }));
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "Writing Projects" })).toBeNull());
  await waitFor(() => expect(document.querySelector(".shelf")?.textContent).toContain("cite:Project B"));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await waitFor(() => expect(invoked).toEqual(["cite:Project A", "cite:Project B"]));
  await waitFor(() => expect(delivered).toEqual([{ text: "cite:Project B", delivery: 29 }]));
  expect(hidden).toBe(1);
  await act(async () => { oldCopy.resolve({ text: "cite:Project A", warnings: [], delivery: 17 }); });
  expect(hidden).toBe(1);
  expect(delivered).toEqual([{ text: "cite:Project B", delivery: 29 }]);
  expect(document.querySelector(".shelf")?.textContent).toContain("cite:Project B");
  expect(document.querySelector(".shelf")?.textContent).not.toContain("cite:Project A");
});

test("a checked bibliography warning still copies the waiting citation once and keeps the picker open", async () => {
  let destination = writingDestination("Project A", "cite", [1, 0]);
  const earlier = pending<FilePreview>(), checked = pending<FilePreview>();
  const warning = "synthetic2026 is missing from refs.bib.";
  const copied: { text: string; warnings: string[] }[] = [];
  let earlierStarted = false, checkedStarted = false, hidden = 0;
  backend({
    search: () => [result("a", "Synthetic source")], picker_destination: () => destination,
    preview_file_citation: (request) => {
      if (request.intent.context?.[1] === 1) { checkedStarted = true; return checked.promise; }
      if (request.intent.context?.[0] === 1) { earlierStarted = true; return earlier.promise; }
      return receipt("initial context");
    },
    copy_file_citation: (args) => {
      const { preview } = prepared(args);
      const outcome = { text: preview.text, warnings: preview.warnings };
      copied.push(outcome);
      return outcome;
    },
    hide_picker: () => { hidden += 1; return true; },
  });
  render(<App />);
  await screen.findByText("Synthetic source");
  await waitFor(() => expect(earlierStarted).toBe(true));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  destination = writingDestination("Project A", "cite", [1, 1]);
  await act(async () => { await emit("project:changed", [1, 1]); });
  await waitFor(() => expect(checkedStarted).toBe(true));
  await act(async () => { checked.resolve(receipt("cite:checked warning", null, [warning])); });
  await waitFor(() => expect(copied).toEqual([{ text: "cite:checked warning", warnings: [warning] }]));
  await waitFor(() => expect(document.body.textContent).toContain(warning));
  expect(hidden).toBe(0);
  expect(screen.getByLabelText("Search references")).toBeTruthy();
  await act(async () => { earlier.resolve(receipt("cite:unchecked older citation")); });
  expect(copied).toEqual([{ text: "cite:checked warning", warnings: [warning] }]);
  expect(hidden).toBe(0);
});

test("a bibliography event retires an old copy proof before its delayed destination read settles", async () => {
  const first = writingDestination("Project A", "cite", [1, 0]);
  const refreshed = writingDestination("Project A", "cite", [1, 1]);
  const destination = pending<PickerDestination>(), checked = pending<FilePreview>();
  const oldCopy = pending<{ text: string; warnings: string[]; delivery: number }>();
  const invoked: string[] = [], delivered: { text: unknown; delivery: unknown }[] = [];
  let reads = 0, checkedPreviewStarted = false, hidden = 0;
  backend({
    search: () => [result("a", "Synthetic source")],
    picker_destination: () => ++reads === 1 ? first : destination.promise,
    preview_file_citation: (request) => {
      if (request.intent.context?.[1] === 1) { checkedPreviewStarted = true; return checked.promise; }
      return receipt("cite:before bibliography check");
    },
    copy_file_citation: (args) => {
      const text = prepared(args).preview.text;
      invoked.push(text);
      return text === "cite:before bibliography check" ? oldCopy.promise : { text, warnings: [], delivery: 29 };
    },
    hide_picker: () => { hidden += 1; return true; },
    show_copied_notice: ({ text, delivery }) => { delivered.push({ text, delivery }); return null; },
  });
  render(<App />);
  await screen.findByText("Synthetic source");
  await waitFor(() => expect(document.querySelector(".shelf")?.textContent).toContain("cite:before bibliography check"));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await waitFor(() => expect(invoked).toEqual(["cite:before bibliography check"]));
  await act(async () => { await emit("project:changed", [1, 1]); });
  await waitFor(() => expect(reads).toBe(2));
  // The event itself retires the earlier context. A native reply cannot use
  // the interval before the refreshed destination arrives to deliver its proof.
  await act(async () => { oldCopy.resolve({ text: "cite:before bibliography check", warnings: [], delivery: 17 }); });
  expect(hidden).toBe(0);
  expect(delivered).toEqual([]);
  await act(async () => { destination.resolve(refreshed); });
  await waitFor(() => expect(checkedPreviewStarted).toBe(true));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await act(async () => { checked.resolve(receipt("cite:current checked context [1,1]")); });
  await waitFor(() => expect(invoked).toEqual(["cite:before bibliography check", "cite:current checked context [1,1]"]));
  await waitFor(() => expect(delivered).toEqual([{ text: "cite:current checked context [1,1]", delivery: 29 }]));
  expect(hidden).toBe(1);
});
