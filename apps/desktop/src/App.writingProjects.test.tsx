import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import type { PickerDestination, WritingProjectsView } from "./api";
import { backend, prepared, receipt, resetApp, result } from "./testing/app";

afterEach(resetApp);

test("leaving a writing project restores the app's output and copies a fresh preview", async () => {
  const project = { id: "synthetic-project", name: "Field Guide", revision: "r1", output: { key: "cite" } as const, bibliography: null };
  let generation = 1;
  let selected: string | null = project.id;
  const view = (): WritingProjectsView => ({ projects: [project], selected, context: `opening-${generation}`, session_only: false });
  const destination = (): PickerDestination => ({ app: "Example Editor", app_id: "example.editor", app_identity: null,
    automatic: null, remembered: { key: "typst" }, unreadable: false, can_paste: false, output_question: "editor",
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
      await emit("project:changed");
      return view();
    },
    preview_file_citation: (request) => receipt(`${request.format}:${JSON.stringify(request.intent.context)}`),
    copy_file_citation: (args) => { const text = prepared(args).preview.text; copied.push(text); return { text, warnings: [] }; },
  });
  render(<App />);
  await screen.findByText("Synthetic source");
  const formatButton = () => within(screen.getByLabelText("Search references").closest("header")!).getByRole("button", { name: /^Copy as / });
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
