import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { AttachmentTextLibrary } from "../bindings";
import { testStates } from "../test-fixtures";
import { deferred } from "../testing/deferred";
import { AttachmentText, type AttachmentTextApi } from "./AttachmentText";

afterEach(cleanup);

const library: AttachmentTextLibrary = { id: "lib_invented_library", name: "Invented references", on: false };
const label = "Let AI apps read attachment text";
const toggle = () => screen.getByRole<HTMLInputElement>("switch", { name: `${label} ${library.name}` });

function group(overrides: Partial<AttachmentTextApi> = {}) {
  let rows = [library];
  const api: AttachmentTextApi = {
    attachmentTextLibraries: vi.fn(async () => rows),
    setAttachmentText: vi.fn(async (id, on) => { rows = rows.map((row) => row.id === id ? { ...row, on } : row); return on; }),
    onSettingsChanged: async () => () => {},
    ...overrides,
  };
  render(<AttachmentText api={api} />);
  return api;
}

test("renders a switch per open library, with names, saved choices and the reading boundary", async () => {
  group({ attachmentTextLibraries: async () => [library, { id: "lib_other_library", name: "Other references", on: true }] });
  expect(await screen.findByRole("heading", { name: "Attachment text" })).toBeTruthy();
  await waitFor(() => expect(screen.getAllByRole("switch")).toHaveLength(2));
  expect(toggle().checked).toBe(false);
  expect(screen.getByRole("switch", { name: `${label} Other references` })).toHaveProperty("checked", true);
  expect(document.body.textContent).toContain("with page numbers, never the files");
  expect(document.body.textContent).toContain("ibis read in a terminal does not need this choice");
});

test("toggles only the returned LibraryId and restores the saved choice when a save fails", async () => {
  const api = group();
  await screen.findByRole("switch");
  fireEvent.click(toggle());
  await waitFor(() => expect(api.setAttachmentText).toHaveBeenCalledWith(library.id, true));
  expect(toggle().checked).toBe(true);
  vi.mocked(api.setAttachmentText).mockRejectedValueOnce(new Error("invented refusal"));
  fireEvent.click(toggle());
  await screen.findByRole("alert");
  expect(toggle().checked).toBe(true);
});

test.each(["success", "failure"])("a late save's %s cannot overwrite a newer choice", async (outcome) => {
  const older = deferred<boolean>();
  const api = group({ setAttachmentText: vi.fn().mockReturnValueOnce(older.promise).mockResolvedValueOnce(false) });
  await screen.findByRole("switch");
  fireEvent.click(toggle());
  fireEvent.click(toggle());
  await waitFor(() => expect(api.setAttachmentText).toHaveBeenLastCalledWith(library.id, false));
  await act(async () => { if (outcome === "success") older.resolve(true); else older.reject(new Error("late refusal")); });
  expect(toggle().checked).toBe(false);
  expect(screen.queryByRole("alert")).toBeNull();
});

test("a late library read cannot overwrite a newer switch", async () => {
  const older = deferred<AttachmentTextLibrary[]>();
  group({ attachmentTextLibraries: vi.fn().mockResolvedValue([{ ...library, on: true }]).mockResolvedValueOnce([library]).mockReturnValueOnce(older.promise) });
  await screen.findByRole("switch");
  fireEvent.focus(window);
  fireEvent.click(toggle());
  await act(async () => older.resolve([library]));
  expect(toggle().checked).toBe(true);
});

test("no open library shows one empty row and no switch", async () => {
  group({ attachmentTextLibraries: async () => [] });
  expect(await screen.findByText("No library is open.")).toBeTruthy();
  expect(screen.queryByRole("switch")).toBeNull();
});

test("a library closed during a pending save is refreshed after the save", async () => {
  const saving = deferred<boolean>();
  let changed!: () => void;
  const api = group({
    attachmentTextLibraries: vi.fn().mockResolvedValueOnce([library]).mockResolvedValue([]),
    setAttachmentText: () => saving.promise,
    onLibraryChanged: async (listener) => { changed = () => listener(testStates[0]!.library); return () => {}; },
  });
  await screen.findByRole("switch");
  fireEvent.click(toggle());
  act(() => changed());
  expect(api.attachmentTextLibraries).toHaveBeenCalledTimes(1);
  await act(async () => saving.resolve(true));
  expect(await screen.findByText("No library is open.")).toBeTruthy();
});
