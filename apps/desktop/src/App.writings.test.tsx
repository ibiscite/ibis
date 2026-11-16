import { fixtureDestination, formatAnswer } from "./testing/format";
import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import type { PickerDestination, WritingsView } from "./api";
import type { CitedWarning, Preview } from "./bindings";
import { backend, prepared, receipt, resetApp, result } from "./testing/app";

afterEach(resetApp);

const pending = <T,>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
};
const writingDestination = (name: string, format: "cite" | "typst", context: [number, number]): PickerDestination => (fixtureDestination({
  app: "Example Editor", app_id: "example.editor", app_identity: null, unreadable: false, can_paste: false, paste: null, paste_question: false, can_choose_paste: false,
  writing: { id: name, name, output: format, bibliography: "refs.bib" },
  writing_context: `opening-${context[0]}`, context_version: context, format: formatAnswer(format, "writing") }));


test("a bibliography read finishing for the same writing refreshes a waiting Enter without cancelling it", async () => {
  let destination = writingDestination("Writing A", "cite", [1, 0]);
  const earlier = pending<Preview>(), checked = pending<Preview>();
  const copied: string[] = [];
  let delayedPreviewStarted = false, checkedPreviewStarted = false;
  backend({
    search: () => [result("a", "Synthetic source")], "destination": () => destination,
    preview_file_citation: (request) => {
      if (request.intent.context?.[1] === 1) { checkedPreviewStarted = true; return checked.promise; }
      if (request.intent.context?.[0] === 1) { delayedPreviewStarted = true; return earlier.promise; }
      return receipt("initial context");
    },
    deliver_citation: (args) => { const text = prepared(args).preview.text; copied.push(text); return { outcome: "retained", copied: { text, warnings: [] }, reason: "pinned" }; },
  });
  render(<App />);
  await screen.findByText("Synthetic source");
  await waitFor(() => expect(delayedPreviewStarted).toBe(true));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  expect(copied).toEqual([]);
  destination = writingDestination("Writing A", "cite", [1, 1]);
  await act(async () => { await emit("project:changed", [1, 1]); });
  await waitFor(() => expect(checkedPreviewStarted).toBe(true));
  await act(async () => { checked.resolve(receipt("cite:[1,1] checked against refs.bib")); });
  await waitFor(() => expect(copied).toEqual(["cite:[1,1] checked against refs.bib"]));
  await act(async () => { earlier.resolve(receipt("cite:[1,0] before bibliography read")); });
  expect(copied).toEqual(["cite:[1,1] checked against refs.bib"]);
});

test("leaving a writing restores the app's output and copies a fresh preview", async () => {
  const writing = { id: "synthetic-project", name: "Field Guide", output: "cite", bibliography: null };
  let generation = 1;
  let selected: string | null = writing.id;
  const view = (): WritingsView => ({ writings: [writing], selected, context: `opening-${generation}`, session_only: false });
  const destination = (): PickerDestination => (fixtureDestination({ app: "Example Editor", app_id: "example.editor", app_identity: null,     unreadable: false, can_paste: false, paste: null, paste_question: false, can_choose_paste: false, writing: selected ? writing : null,
    writing_context: `opening-${generation}`, context_version: [generation, 0], format: formatAnswer(selected ? writing.output : "typst", selected ? "writing" : "app") }));
  const copied: string[] = [];
  backend({
    search: () => [result("a", "Synthetic source")], "destination": destination, writings: view,
    "edit-writing": async ({ context, action }) => {
      expect(context).toBe(`opening-${generation}`);
      expect(action).toEqual({ kind: "select", id: null });
      selected = null;
      generation += 1;
      await emit("project:changed", [generation, 0]);
      return view();
    },
    preview_file_citation: (request) => receipt(`${request.output}:${JSON.stringify(request.intent.context)}`),
    deliver_citation: (args) => { const text = prepared(args).preview.text; copied.push(text); return { outcome: "retained", copied: { text, warnings: [] }, reason: "pinned" }; },
  });
  render(<App />);
  await screen.findByText("Synthetic source");
  const formatButton = () => within(screen.getByLabelText("Search references").closest("header")!).getByRole("button", { name: /^Citation format / });
  await waitFor(() => expect(formatButton().getAttribute("aria-label")).toContain("Field Guide"));
  fireEvent.click(formatButton());
  fireEvent.click(await screen.findByRole("option", { name: "New writing…" }));
  const dialog = await screen.findByRole("dialog", { name: "Writings" });
  fireEvent.click(await within(dialog).findByRole("button", { name: "No Writing" }));
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "Writings" })).toBeNull());
  await waitFor(() => expect(formatButton().getAttribute("aria-label")).toContain("Typst"));
  expect(formatButton().getAttribute("aria-label")).not.toContain("Field Guide");
  await act(async () => { fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" }); });
  await waitFor(() => expect(copied).toEqual(["typst:[2,0]"]));
});

async function openWritings() {
  const format = within(screen.getByLabelText("Search references").closest("header")!).getByRole<HTMLButtonElement>("button", { name: /^Citation format / });
  await waitFor(() => expect(format.disabled).toBe(false));
  fireEvent.click(format);
  fireEvent.click(await screen.findByRole("option", { name: "New writing…" }));
  return screen.findByRole("dialog", { name: "Writings" });
}

test("Back and return keep a sheet's typed draft, selection, scroll and focus", async () => {
  backend({ search: () => [result("a", "Synthetic source")],
    writings: () => ({ writings: [], selected: null, active: null, context: "opening-1", session_only: false }) });
  render(<App />);
  await screen.findByText("Synthetic source");
  await openWritings();
  fireEvent.click(await screen.findByRole("button", { name: "New Writing…" }));
  const field = screen.getByRole<HTMLInputElement>("textbox", { name: "Writing name" });
  fireEvent.change(field, { target: { value: "Unfinished field guide" } });
  act(() => { field.focus(); field.setSelectionRange(3, 8); });
  const body = document.querySelector<HTMLElement>(".ui-sheet-body")!;
  body.scrollTop = 120;
  fireEvent.keyDown(field, { key: "ArrowLeft", altKey: true });
  expect(screen.queryByRole("dialog", { name: "Writings" })).toBeNull();
  await openWritings();
  const restored = await screen.findByRole<HTMLInputElement>("textbox", { name: "Writing name" });
  expect(restored.value).toBe("Unfinished field guide");
  expect(document.activeElement).toBe(restored);
  expect([restored.selectionStart, restored.selectionEnd]).toEqual([3, 8]);
  expect(document.querySelector<HTMLElement>(".ui-sheet-body")!.scrollTop).toBe(120);
});

test("a fresh Enter after selecting a writing waits for its delayed destination and copies the new format once", async () => {
  const first = writingDestination("Writing A", "cite", [1, 0]);
  const second = writingDestination("Writing B", "typst", [2, 0]);
  const writings = [first.writing!, second.writing!];
  const save = pending<WritingsView>(), destinationRead = pending<PickerDestination>();
  let saved = false, destinationReadStarted = false;
  const copied: string[] = [];
  const view = (): WritingsView => ({ writings: writings, selected: saved ? "Writing B" : "Writing A", context: saved ? "opening-2" : "opening-1", session_only: false });
  backend({
    search: () => [result("a", "Synthetic source")],
    "destination": () => { if (!saved) return first; destinationReadStarted = true; return destinationRead.promise; },
    writings: view,
    "edit-writing": async ({ context, action }) => {
      expect(context).toBe("opening-1");
      expect(action).toEqual({ kind: "select", id: "Writing B" });
      const next = await save.promise;
      saved = true;
      await emit("project:changed", [2, 0]);
      return next;
    },
    preview_file_citation: (request) => receipt(`${request.output}:${JSON.stringify(request.intent.context)}`),
    deliver_citation: (args) => { const text = prepared(args).preview.text; copied.push(text); return { outcome: "retained", copied: { text, warnings: [] }, reason: "pinned" }; },
  });
  render(<App />);
  await screen.findByText("Synthetic source");
  const dialog = await openWritings();
  fireEvent.click(await within(dialog).findByRole("button", { name: "Use Writing B" }));
  expect(screen.getByRole("dialog", { name: "Writings" })).toBeTruthy();
  await act(async () => { save.resolve({ ...view(), selected: "Writing B", context: "opening-2" }); });
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "Writings" })).toBeNull());
  await waitFor(() => expect(destinationReadStarted).toBe(true));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  expect(copied).toEqual([]);
  await act(async () => { destinationRead.resolve(second); });
  await waitFor(() => expect(copied).toEqual(["typst:[2,0]"]));
});

test("a same-format writing switch starts a new copy and an older copy cannot deliver its proof or replace the receipt", async () => {
  const first = writingDestination("Writing A", "cite", [1, 0]);
  const second = writingDestination("Writing B", "cite", [2, 0]);
  const writings = [first.writing!, second.writing!];
  const oldCopy = pending<import("./bindings").DeliveryOutcome>();
  let selected = "Writing A";
  const invoked: string[] = [];
  const view = (): WritingsView => ({ writings: writings, selected, context: selected === "Writing A" ? "opening-1" : "opening-2", session_only: false });
  backend({
    search: () => [result("a", "Synthetic source")],
    "destination": () => selected === "Writing A" ? first : second,
    writings: view,
    "edit-writing": async () => { selected = "Writing B"; await emit("project:changed", [2, 0]); return view(); },
    preview_file_citation: (request) => receipt(request.intent.context?.[0] === 2 ? "cite:Writing B" : "cite:Writing A"),
    deliver_citation: (args) => {
      const text = prepared(args).preview.text;
      invoked.push(text);
      return text === "cite:Writing A" ? oldCopy.promise : { outcome: "retained", copied: { text, warnings: [] }, reason: "pinned" };
    },
  });
  render(<App />);
  await screen.findByText("Synthetic source");
  await waitFor(() => expect(document.querySelector(".shelf")?.textContent).toContain("cite:Writing A"));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await waitFor(() => expect(invoked).toEqual(["cite:Writing A"]));
  const dialog = await openWritings();
  fireEvent.click(await within(dialog).findByRole("button", { name: "Use Writing B" }));
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "Writings" })).toBeNull());
  await waitFor(() => expect(document.querySelector(".shelf")?.textContent).toContain("cite:Writing B"));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await waitFor(() => expect(invoked).toEqual(["cite:Writing A", "cite:Writing B"]));
  await act(async () => { oldCopy.resolve({ outcome: "retained", copied: { text: "cite:Writing A", warnings: [] }, reason: "pinned" }); });
  expect(document.querySelector(".shelf")?.textContent).toContain("cite:Writing B");
  expect(document.querySelector(".shelf")?.textContent).not.toContain("cite:Writing A");
});

test("a checked bibliography warning still copies the waiting citation once and keeps the picker open", async () => {
  let destination = writingDestination("Writing A", "cite", [1, 0]);
  const earlier = pending<Preview>(), checked = pending<Preview>();
  const warning = { item: 0, message: { id: "app-project-warning", args: { key: "synthetic2026", name: "refs.bib", state: "missing" } } };
  const copied: { text: string; warnings: CitedWarning[] }[] = [];
  let earlierStarted = false, checkedStarted = false, hidden = 0;
  backend({
    search: () => [result("a", "Synthetic source")], "destination": () => destination,
    preview_file_citation: (request) => {
      if (request.intent.context?.[1] === 1) { checkedStarted = true; return checked.promise; }
      if (request.intent.context?.[0] === 1) { earlierStarted = true; return earlier.promise; }
      return receipt("initial context");
    },
    deliver_citation: (args) => {
      const { preview } = prepared(args);
      const outcome = { text: preview.text, warnings: preview.warnings };
      copied.push(outcome);
      return { outcome: "retained", copied: outcome, reason: "warnings" };
    },
    hide_picker: () => { hidden += 1; return true; },
  });
  render(<App />);
  await screen.findByText("Synthetic source");
  await waitFor(() => expect(earlierStarted).toBe(true));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  destination = writingDestination("Writing A", "cite", [1, 1]);
  await act(async () => { await emit("project:changed", [1, 1]); });
  await waitFor(() => expect(checkedStarted).toBe(true));
  await act(async () => { checked.resolve(receipt("cite:checked warning", null, [warning])); });
  await waitFor(() => expect(copied).toEqual([{ text: "cite:checked warning", warnings: [warning] }]));
  await waitFor(() => expect(document.body.textContent).toContain("synthetic2026 is missing from refs.bib."));
  expect(hidden).toBe(0);
  expect(screen.getByLabelText("Search references")).toBeTruthy();
  await act(async () => { earlier.resolve(receipt("cite:unchecked older citation")); });
  expect(copied).toEqual([{ text: "cite:checked warning", warnings: [warning] }]);
  expect(hidden).toBe(0);
});

test("a bibliography event retires an old copy proof before its delayed destination read settles", async () => {
  const first = writingDestination("Writing A", "cite", [1, 0]);
  const refreshed = writingDestination("Writing A", "cite", [1, 1]);
  const destination = pending<PickerDestination>(), checked = pending<Preview>();
  const oldCopy = pending<import("./bindings").DeliveryOutcome>();
  const invoked: string[] = [];
  let reads = 0, checkedPreviewStarted = false;
  backend({
    search: () => [result("a", "Synthetic source")],
    "destination": () => ++reads === 1 ? first : destination.promise,
    preview_file_citation: (request) => {
      if (request.intent.context?.[1] === 1) { checkedPreviewStarted = true; return checked.promise; }
      return receipt("cite:before bibliography check");
    },
    deliver_citation: (args) => {
      const text = prepared(args).preview.text;
      invoked.push(text);
      return text === "cite:before bibliography check" ? oldCopy.promise : { outcome: "retained", copied: { text, warnings: [] }, reason: "pinned" };
    },
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
  await act(async () => { oldCopy.resolve({ outcome: "retained", copied: { text: "cite:before bibliography check", warnings: [] }, reason: "pinned" }); });
  await act(async () => { destination.resolve(refreshed); });
  await waitFor(() => expect(checkedPreviewStarted).toBe(true));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await act(async () => { checked.resolve(receipt("cite:current checked context [1,1]")); });
  await waitFor(() => expect(invoked).toEqual(["cite:before bibliography check", "cite:current checked context [1,1]"]));
});
