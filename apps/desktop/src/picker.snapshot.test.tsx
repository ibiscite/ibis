import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { afterEach, expect, test, vi } from "vitest";
import { App } from "./App";
import type { LibraryStatus, PickerSnapshot } from "./api";
import { backend, resetApp, result } from "./testing/app";
import { deferred } from "./testing/deferred";
import { fixtureDestination, formatAnswer } from "./testing/format";

afterEach(resetApp);

function snapshot(presentation: number, title: string): PickerSnapshot {
  const revision = String(presentation);
  const row = { ...result(`snapshot-${presentation}`, title), revision, recent: "cited" as const };
  return {
    presentation, selection: null,
    library: { revision, source_generation: revision, source_is_requested: true, state: "loaded", has_snapshot: true,
      name: "invented.bib", snapshot_name: "invented.bib", references: 1, warnings: 0, error: null } as LibraryStatus,
    recents: { Ok: [{ revision, results: [row] }, true, false] },
    destination: { Ok: fixtureDestination({ app: "Invented Writer", app_id: "invented.writer", can_paste: true, format: formatAnswer("typst") }) },
    preferences: { Ok: { output: "apa", density: "compact", larger_text: true, paste: true, menu_bar: true, appearance: "dark", accent: "ibis" } },
  };
}

test("the first frame uses every snapshot source without startup fetches", async () => {
  const painted = vi.fn(() => ({
    row: screen.queryByText("First reference")?.textContent,
    format: screen.queryByRole("button", { name: /^Citation format.*Typst/ })?.getAttribute("aria-label"),
    density: document.documentElement.dataset.density,
    textSize: document.documentElement.dataset.textSize,
    appearance: document.documentElement.dataset.appearance,
    accent: document.documentElement.dataset.accent,
  }));
  const calls = backend({ picker_snapshot: () => snapshot(1, "First reference"), picker_rendered: painted });
  render(<App />);
  await waitFor(() => expect(painted).toHaveBeenCalledOnce());
  expect(painted.mock.results[0]?.value).toMatchObject({ row: "First reference", format: expect.stringContaining("Typst"),
    density: "compact", textSize: "larger", appearance: "dark", accent: "ibis" });
  for (const command of ["library_status", "recent_references", "picker_destination", "picker_preferences"]) expect(calls).not.toContain(command);
});

test("a picker opened while its library loads shows the loaded count and rows without reopening", async () => {
  const opening = snapshot(1, "Unused opening row");
  opening.library = { ...opening.library, state: "loading", has_snapshot: false, references: 0 };
  opening.recents = { Ok: [{ revision: "1", results: [] }, false, false] };
  const loaded = snapshot(2, "Loaded reference");
  loaded.library.references = 3;
  const painted = vi.fn();
  backend({ picker_snapshot: () => opening, picker_rendered: painted, recent_references: () => {
    if (!("Ok" in loaded.recents)) throw new Error("Missing invented recents");
    return loaded.recents.Ok;
  } });
  render(<App />);
  await waitFor(() => expect(painted).toHaveBeenCalledOnce());
  expect(screen.queryByText("Loaded reference")).toBeNull();
  await act(async () => { await emit("library:changed", loaded.library); });
  await screen.findByText("Loaded reference");
  expect(document.querySelector(".library-detail")?.textContent).toContain("3 references");
  expect(painted).toHaveBeenCalledOnce();
});

test("a newer snapshot replaces the current frame and beats a late startup snapshot", async () => {
  const startup = deferred<PickerSnapshot>();
  const painted = vi.fn();
  backend({ picker_snapshot: () => startup.promise, picker_rendered: painted });
  render(<App />);
  await act(async () => { await emit("picker:preparing", snapshot(2, "New reference")); });
  expect(screen.getByText("New reference")).toBeTruthy();
  await act(async () => { startup.resolve(snapshot(1, "Old reference")); });
  expect(screen.queryByText("Old reference")).toBeNull();
  await act(async () => { await emit("picker:preparing", snapshot(3, "Newest reference")); });
  expect(screen.getByText("Newest reference")).toBeTruthy();
  expect(screen.queryByText("New reference")).toBeNull();
  expect(painted.mock.calls.map(([args]) => args.presentation)).toEqual([2, 3]);
});

test("reopening clears the old rows and preview and rejects their late answers", async () => {
  const searching = deferred<ReturnType<typeof result>[]>();
  const currentSearch = deferred<ReturnType<typeof result>[]>();
  const painted = vi.fn();
  backend({ picker_snapshot: () => snapshot(1, "Old reference"), picker_rendered: painted, search: ({ query }) => query.text === "old query" ? searching.promise : currentSearch.promise });
  render(<App />);
  await screen.findByText("Old reference");
  await waitFor(() => expect(screen.getByRole("region", { name: "Preview" }).textContent).toContain("@snapshot-1"));
  const input = screen.getByLabelText("Search references");
  fireEvent.change(input, { target: { value: "old query" } });
  const next = snapshot(2, "New reference");
  next.selection = "new query";
  await act(async () => { await emit("picker:hidden"); await emit("picker:preparing", next); });
  expect(screen.queryByText("Old reference")).toBeNull();
  expect(screen.getByLabelText<HTMLInputElement>("Search references").value).toBe("new query");
  expect(screen.queryByRole("region", { name: "Preview" })?.textContent ?? "").not.toContain("@snapshot-1");
  // This old flight must not donate rows to the new opening.
  await act(async () => { searching.resolve([result("stale", "Stale late reference")]); });
  expect(screen.queryByText("Old reference")).toBeNull();
  expect(screen.queryByText("Stale late reference")).toBeNull();
  expect(painted).toHaveBeenCalledTimes(2);
});

test.each(["invented.writer", "other.writer"])("a new destination observation for %s retires the opening's recents on the same library revision", async (appId) => {
  const first = snapshot(1, "First app reference");
  const nextDestination = fixtureDestination({ app: "Other Writer", app_id: appId, format: formatAnswer("typst") });
  const recents = vi.fn(() => [{ revision: "1", results: [result("other", "Other app reference")] }, true, false]);
  backend({ picker_snapshot: () => first, picker_destination: () => nextDestination, recent_references: recents });
  render(<App />);
  await screen.findByText("First app reference");
  expect(recents).not.toHaveBeenCalled();
  await act(async () => { await emit("writings:changed"); });
  await screen.findByText("Other app reference");
  expect(screen.queryByText("First app reference")).toBeNull();
  expect(recents).toHaveBeenCalledOnce();
});

test("clearing a later query reads current recents instead of reusing the opening rows", async () => {
  const recents = vi.fn(() => [{ revision: "1", results: [result("fresh", "Fresh recent reference")] }, true, false]);
  backend({ picker_snapshot: () => snapshot(1, "Opening reference"), recent_references: recents, search: () => [] });
  render(<App />);
  await screen.findByText("Opening reference");
  const input = screen.getByLabelText("Search references");
  fireEvent.change(input, { target: { value: "no match" } });
  await screen.findByText("No matching references");
  fireEvent.change(input, { target: { value: "" } });
  await screen.findByText("Fresh recent reference");
  expect(screen.queryByText("Opening reference")).toBeNull();
  expect(recents).toHaveBeenCalledOnce();
});
