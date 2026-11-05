import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { WritingProjectsView } from "../../api";
import { closeOverlays } from "../../ui/layers";
import { WritingProjectsDialog, type WritingProjectsApi } from "./WritingProjects";

afterEach(cleanup);

const thesis = { id: "thesis", name: "Thesis", output: "autocite", bibliography: "thesis.bib", revision: "1" };
const notes = { id: "notes", name: "Notes", output: "pandoc", bibliography: null, revision: "1" };
const view = (selected: string | null = "thesis"): WritingProjectsView => ({ projects: [thesis, notes], selected, active: [thesis, notes].find((project) => project.id === selected) ?? null, context: "opening-a", session_only: false });
function api(overrides: Partial<WritingProjectsApi> = {}): WritingProjectsApi {
  return { writingProjects: async () => view(), editWritingProject: async () => view(), ...overrides };
}

test("selecting a project waits for its save and then returns to the picker", async () => {
  let finish!: (next: WritingProjectsView) => void;
  const edit = vi.fn(() => new Promise<WritingProjectsView>((resolve) => { finish = resolve; }));
  const close = vi.fn();
  render(<WritingProjectsDialog api={api({ editWritingProject: edit })} format="autocite" onClose={close} />);
  fireEvent.click(await screen.findByRole("button", { name: "Use Notes" }));
  expect(edit).toHaveBeenCalledWith("opening-a", { kind: "select", id: "notes" });
  expect(close).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "Use Thesis" }).getAttribute("aria-pressed")).toBe("true");
  await act(async () => { finish(view("notes")); });
  expect(close).toHaveBeenCalledOnce();
});

test("failed saves preserve the selected project and explain the failure", async () => {
  const close = vi.fn();
  render(<WritingProjectsDialog api={api({ editWritingProject: async () => { throw new Error("synthetic failure"); } })} format="autocite" onClose={close} />);
  fireEvent.click(await screen.findByRole("button", { name: "Use Notes" }));
  await screen.findByRole("alert");
  expect(screen.getByRole("button", { name: "Use Thesis" }).getAttribute("aria-pressed")).toBe("true");
  expect(close).not.toHaveBeenCalled();
});

test("an older read cannot replace a successful edit", async () => {
  let changed!: () => void;
  let late!: (next: WritingProjectsView) => void;
  let delayed = false;
  render(<WritingProjectsDialog api={api({
    writingProjects: () => delayed ? new Promise((resolve) => { late = resolve; }) : Promise.resolve(view()),
    onProjectChanged: async (listen) => { changed = listen; return () => undefined; },
    editWritingProject: async () => ({ ...view(), projects: [thesis, { ...notes, name: "Field Notes", revision: "2" }] }),
  })} format="autocite" onClose={vi.fn()} />);
  await screen.findByRole("button", { name: "Use Notes" });
  delayed = true;
  act(() => changed());
  fireEvent.click(screen.getByRole("button", { name: "Rename Project…" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Project name" }), { target: { value: "Research" } });
  fireEvent.click(screen.getByRole("button", { name: "Save Name" }));
  await screen.findByRole("button", { name: "Use Field Notes" });
  await act(async () => { late(view()); });
  expect(screen.queryByRole("button", { name: "Use Notes" })).toBeNull();
});

test("creating a project uses the current format and refuses an empty name", async () => {
  const edit = vi.fn(async () => ({ ...view("notes"), projects: [notes] }));
  render(<WritingProjectsDialog api={api({ editWritingProject: edit })} format="pandoc" onClose={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "New Writing Project…" }));
  expect(screen.getByRole<HTMLButtonElement>("button", { name: "Create Project" }).disabled).toBe(true);
  fireEvent.change(screen.getByRole("textbox", { name: "Project name" }), { target: { value: "  Notes  " } });
  fireEvent.click(screen.getByRole("button", { name: "Create Project" }));
  await waitFor(() => expect(edit).toHaveBeenCalledWith("opening-a", { kind: "create", name: "Notes", output: "pandoc" }));
  await screen.findByText("No bibliography selected.");
});

test("removing a project requires an explicit choice and includes its revision", async () => {
  const edit = vi.fn(async () => view(null));
  render(<WritingProjectsDialog api={api({ editWritingProject: edit })} format="autocite" onClose={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "Remove Project…" }));
  expect(edit).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Remove Project" }));
  await waitFor(() => expect(edit).toHaveBeenCalledWith("opening-a", { kind: "remove", id: "thesis", revision: "1" }));
});

test("a refused project name keeps the form and the native correction", async () => {
  render(<WritingProjectsDialog api={api({ editWritingProject: async () => { throw "A project already has that name. Choose another name."; } })} format="autocite" onClose={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "New Writing Project…" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Project name" }), { target: { value: "Thesis" } });
  fireEvent.click(screen.getByRole("button", { name: "Create Project" }));
  expect((await screen.findByRole("alert")).textContent).toBe("A project already has that name. Choose another name.");
  expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Project name" }).value).toBe("Thesis");
  expect(screen.getByRole("button", { name: "Use Thesis" }).getAttribute("aria-pressed")).toBe("true");
});

test("an unidentified app explains that project selection is session only", async () => {
  render(<WritingProjectsDialog api={api({ writingProjects: async () => ({ ...view(), session_only: true }) })} format="autocite" onClose={vi.fn()} />);
  await screen.findByText("Ibis can’t identify the app, so this project applies to this session only.");
});

test("hiding the picker closes the sheet and a late save cannot close its replacement", async () => {
  let finish!: (next: WritingProjectsView) => void;
  const closed = vi.fn();
  const rendered = render(<WritingProjectsDialog api={api({ editWritingProject: () => new Promise((resolve) => { finish = resolve; }) })} format="autocite" onClose={closed} />);
  fireEvent.click(await screen.findByRole("button", { name: "Use Notes" }));
  act(closeOverlays);
  expect(closed).toHaveBeenCalledOnce();
  await act(async () => { finish(view("notes")); });
  expect(closed).toHaveBeenCalledOnce();
  rendered.unmount();
});

test("external settings stay pending until Apply Updates saves and the active settings remain visible", async () => {
  const latest = { ...thesis, output: "pandoc", bibliography: "paper.bib", revision: "2" };
  const current = { ...view(), projects: [latest, notes], active: { ...thesis, pending: latest, retained: false } };
  let finish!: (next: WritingProjectsView) => void;
  const edit = vi.fn(() => new Promise<WritingProjectsView>((resolve) => { finish = resolve; }));
  render(<WritingProjectsDialog api={api({ writingProjects: async () => current, editWritingProject: edit })} format="autocite" onClose={vi.fn()} />);
  const apply = await screen.findByRole("button", { name: "Apply Updates" });
  expect(screen.getByRole("region", { name: "Selected writing project" }).textContent).toContain("thesis.bib");
  fireEvent.click(apply);
  expect(edit).toHaveBeenCalledWith("opening-a", { kind: "apply", id: "thesis", revision: "2" });
  expect(screen.getByRole("region", { name: "Selected writing project" }).textContent).toContain("thesis.bib");
  await act(async () => { finish({ ...view(), projects: [latest, notes], active: latest }); });
  await waitFor(() => expect(screen.queryByRole("button", { name: "Apply Updates" })).toBeNull());
  expect(screen.getByRole("region", { name: "Selected writing project" }).textContent).toContain("paper.bib");
});

test("a project deleted elsewhere retains its task settings and is labelled without selecting defaults", async () => {
  const current = { ...view(), projects: [notes], active: { ...thesis, retained: true, pending: null } };
  render(<WritingProjectsDialog api={api({ writingProjects: async () => current })} format="autocite" onClose={vi.fn()} />);
  await screen.findByText(/retained settings/i);
  const active = screen.getByRole("region", { name: "Selected writing project" });
  expect(active.textContent).toContain("thesis.bib");
  expect(screen.getByRole("button", { name: "No Writing Project" }).getAttribute("aria-pressed")).toBe("false");
  expect(screen.queryByRole("button", { name: "Apply Updates" })).toBeNull();
});

test("a conflicting rename shows the other writer's settings and preserves the name before an explicit retry", async () => {
  const latest = { ...thesis, name: "Shared Thesis", output: "pandoc", bibliography: "new.bib", revision: "2" };
  let conflicted = false;
  const edit = vi.fn(async (_context, change) => {
    if (!conflicted) { conflicted = true; throw "The writing project changed elsewhere. Review the saved settings before saving again."; }
    expect(change).toEqual({ kind: "rename", id: "thesis", revision: "2", name: "My Thesis" });
    return { ...view(), projects: [{ ...latest, name: "My Thesis", revision: "3" }, notes], active: { ...thesis, name: "My Thesis" } };
  });
  render(<WritingProjectsDialog api={api({
    writingProjects: async () => conflicted ? { ...view(), projects: [latest, notes], active: { ...thesis, name: latest.name, pending: latest } } : { ...view(), active: thesis },
    editWritingProject: edit,
  })} format="autocite" onClose={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "Rename Project…" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Project name" }), { target: { value: "My Thesis" } });
  fireEvent.click(screen.getByRole("button", { name: "Save Name" }));
  const saved = await screen.findByRole("region", { name: "Saved settings from another writer" });
  expect(saved.textContent).toContain("Shared Thesis");
  expect(saved.textContent).toContain("new.bib");
  expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Project name" }).value).toBe("My Thesis");
  expect(edit).toHaveBeenCalledTimes(1);
  fireEvent.click(screen.getByRole("button", { name: "Save My Name" }));
  await waitFor(() => expect(edit).toHaveBeenCalledTimes(2));
});

test("a cosmetic rename updates the label while task citation choices stay unchanged", async () => {
  let changed!: () => void;
  let current = view();
  const edit = vi.fn(async () => current);
  render(<WritingProjectsDialog api={api({ writingProjects: async () => current, editWritingProject: edit,
    onProjectChanged: async (listener) => { changed = listener; return () => undefined; } })} format="autocite" onClose={vi.fn()} />);
  await screen.findByRole("heading", { name: "Thesis" });
  current = { ...view(), active: { ...thesis, name: "Final Thesis" }, projects: [{ ...thesis, name: "Final Thesis", revision: "2" }, notes] };
  await act(async () => { changed(); });
  await screen.findByRole("heading", { name: "Final Thesis" });
  expect(screen.getByRole("region", { name: "Selected writing project" }).textContent).toContain("thesis.bib");
  expect(screen.queryByRole("button", { name: "Apply Updates" })).toBeNull();
  expect(edit).not.toHaveBeenCalled();
});

test("unreadable saved settings keep the task visible without claiming that no projects exist", async () => {
  const message = "Saved project settings could not be checked. Using these task settings. Check Settings and try again.";
  const current = { ...view(), projects: [], active: { ...thesis, saved_problem: message } };
  render(<WritingProjectsDialog api={api({ writingProjects: async () => current })} format="autocite" onClose={vi.fn()} />);
  await screen.findByText((content) => content.includes(message));
  expect(screen.getByRole("region", { name: "Selected writing project" }).textContent).toContain("thesis.bib");
  expect(screen.queryByText(/No writing projects yet/)).toBeNull();
  expect(screen.getByRole<HTMLButtonElement>("button", { name: "New Writing Project…" }).disabled).toBe(true);
});
