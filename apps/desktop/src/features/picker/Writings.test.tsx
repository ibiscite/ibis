import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { WritingsView } from "../../api";
import { WritingsDialog, type WritingsApi } from "./Writings";

afterEach(cleanup);

const thesis = { id: "thesis", name: "Thesis", output: "autocite", bibliography: "thesis.bib" };
const notes = { id: "notes", name: "Notes", output: "pandoc", bibliography: null };
const view = (selected: string | null = "thesis"): WritingsView => ({ writings: [thesis, notes], selected, active: [thesis, notes].find((writing) => writing.id === selected) ?? null, context: "opening-a", session_only: false });
function api(overrides: Partial<WritingsApi> = {}): WritingsApi {
  return { writings: async () => view(), editWriting: async () => view(), ...overrides };
}

test("selecting a writing waits for its save and then returns to the picker", async () => {
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

test("failed saves preserve the selected writing and explain the failure", async () => {
  const close = vi.fn();
  render(<WritingsDialog api={api({ editWriting: async () => { throw new Error("synthetic failure"); } })} format="autocite" onClose={close} />);
  fireEvent.click(await screen.findByRole("button", { name: "Use Notes" }));
  await screen.findByRole("alert");
  expect(screen.getByRole("button", { name: "Use Thesis" }).getAttribute("aria-pressed")).toBe("true");
  expect(close).not.toHaveBeenCalled();
});
