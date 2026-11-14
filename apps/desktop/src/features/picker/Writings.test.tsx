import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { WritingsView } from "../../api";
import { closeOverlays } from "../../ui/layers";
import { WritingsDialog, type WritingsApi } from "./Writings";

afterEach(cleanup);

const thesis = { id: "thesis", name: "Thesis", output: "autocite", bibliography: "thesis.bib" };
const notes = { id: "notes", name: "Notes", output: "pandoc", bibliography: null };
const view = (selected: string | null = "thesis"): WritingsView => ({ writings: [thesis, notes], selected, active: [thesis, notes].find((project) => project.id === selected) ?? null, context: "opening-a", session_only: false });
function api(overrides: Partial<WritingsApi> = {}): WritingsApi {
  return { writings: async () => view(), editWriting: async () => view(), ...overrides };
}

test("selecting a project waits for its save and then returns to the picker", async () => {
  let finish!: (next: WritingsView) => void;
  const edit = vi.fn(() => new Promise<WritingsView>((resolve) => { finish = resolve; }));
  const close = vi.fn();
  render(<WritingsDialog api={api({ editWriting: edit })} format="autocite" onClose={close} />);
  fireEvent.click(await screen.findByRole("button", { name: "Use Notes" }));
  expect(edit).toHaveBeenCalledWith("opening-a", { kind: "select", id: "notes" });
  expect(close).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "Use Thesis" }).getAttribute("aria-pressed")).toBe("true");
  await act(async () => { finish(view("notes")); });
  expect(close).toHaveBeenCalledOnce();
});

test("failed saves preserve the selected project and explain the failure", async () => {
  const close = vi.fn();
  render(<WritingsDialog api={api({ editWriting: async () => { throw new Error("synthetic failure"); } })} format="autocite" onClose={close} />);
  fireEvent.click(await screen.findByRole("button", { name: "Use Notes" }));
  await screen.findByRole("alert");
  expect(screen.getByRole("button", { name: "Use Thesis" }).getAttribute("aria-pressed")).toBe("true");
  expect(close).not.toHaveBeenCalled();
});

test("an older read cannot replace a successful edit", async () => {
  let changed!: () => void;
  let late!: (next: WritingsView) => void;
  let delayed = false;
  render(<WritingsDialog api={api({
    writings: () => delayed ? new Promise((resolve) => { late = resolve; }) : Promise.resolve(view()),
    onProjectChanged: async (listen) => { changed = listen; return () => undefined; },
    editWriting: async () => ({ ...view(), writings: [thesis, { ...notes, name: "Field Notes" }] }),
  })} format="autocite" onClose={vi.fn()} />);
  await screen.findByRole("button", { name: "Use Notes" });
  delayed = true;
  act(() => changed());
  fireEvent.click(screen.getByRole("button", { name: "Rename Writing…" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Writing name" }), { target: { value: "Research" } });
  fireEvent.click(screen.getByRole("button", { name: "Save Name" }));
  await screen.findByRole("button", { name: "Use Field Notes" });
  await act(async () => { late(view()); });
  expect(screen.queryByRole("button", { name: "Use Notes" })).toBeNull();
});

test("creating a project uses the current format and refuses an empty name", async () => {
  const edit = vi.fn(async () => ({ ...view("notes"), writings: [notes] }));
  render(<WritingsDialog api={api({ editWriting: edit })} format="pandoc" onClose={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "New Writing…" }));
  expect(screen.getByRole<HTMLButtonElement>("button", { name: "Create Writing" }).disabled).toBe(true);
  fireEvent.change(screen.getByRole("textbox", { name: "Writing name" }), { target: { value: "  Notes  " } });
  fireEvent.click(screen.getByRole("button", { name: "Create Writing" }));
  await waitFor(() => expect(edit).toHaveBeenCalledWith("opening-a", { kind: "create", name: "Notes", output: "pandoc" }));
  await screen.findByText("No bibliography selected.");
});

test("removing a project requires an explicit choice", async () => {
  const edit = vi.fn(async () => view(null));
  render(<WritingsDialog api={api({ editWriting: edit })} format="autocite" onClose={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "Remove Writing…" }));
  expect(edit).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Remove Writing" }));
  await waitFor(() => expect(edit).toHaveBeenCalledWith("opening-a", { kind: "remove", id: "thesis" }));
});

test("a refused project name keeps the form and the native correction", async () => {
  render(<WritingsDialog api={api({ editWriting: async () => { throw { code: "project-duplicate-name", message: { id: "app-project-error-duplicate-name", args: {} }, recovery: { action: "rename-project" }, focus: "project-name" }; } })} format="autocite" onClose={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "New Writing…" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Writing name" }), { target: { value: "Thesis" } });
  fireEvent.click(screen.getByRole("button", { name: "Create Writing" }));
  expect((await screen.findByRole("alert")).textContent).toBe("A writing already has that name. Choose another name.");
  expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Writing name" }).value).toBe("Thesis");
  expect(screen.getByRole("button", { name: "Use Thesis" }).getAttribute("aria-pressed")).toBe("true");
});

test("an unidentified app explains that project selection is session only", async () => {
  render(<WritingsDialog api={api({ writings: async () => ({ ...view(), session_only: true }) })} format="autocite" onClose={vi.fn()} />);
  await screen.findByText("Ibis can’t identify the app, so this writing applies only until Ibis quits.");
});

test("hiding the picker closes the sheet and a late save cannot close its replacement", async () => {
  let finish!: (next: WritingsView) => void;
  const closed = vi.fn();
  const rendered = render(<WritingsDialog api={api({ editWriting: () => new Promise((resolve) => { finish = resolve; }) })} format="autocite" onClose={closed} />);
  fireEvent.click(await screen.findByRole("button", { name: "Use Notes" }));
  act(closeOverlays);
  expect(closed).toHaveBeenCalledOnce();
  await act(async () => { finish(view("notes")); });
  expect(closed).toHaveBeenCalledOnce();
  rendered.unmount();
});

test("a rename saved elsewhere shows when the projects are read again", async () => {
  let changed!: () => void;
  let current = view();
  const edit = vi.fn(async () => current);
  render(<WritingsDialog api={api({ writings: async () => current, editWriting: edit,
    onProjectChanged: async (listener) => { changed = listener; return () => undefined; } })} format="autocite" onClose={vi.fn()} />);
  await screen.findByRole("heading", { name: "Thesis" });
  current = { ...view(), active: { ...thesis, name: "Final Thesis" }, writings: [{ ...thesis, name: "Final Thesis" }, notes] };
  await act(async () => { changed(); });
  await screen.findByRole("heading", { name: "Final Thesis" });
  expect(screen.getByRole("region", { name: "Selected writing" }).textContent).toContain("thesis.bib");
  expect(edit).not.toHaveBeenCalled();
});
