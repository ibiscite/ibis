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
  let outside = false;
  const api: AttachmentTextApi = {
    attachmentTextLibraries: vi.fn(async () => rows),
    setAttachmentText: vi.fn(async (id, on) => { rows = rows.map((row) => row.id === id ? { ...row, on } : row); return on; }),
    attachmentTextOutsideLibrary: vi.fn(async () => outside),
    setAttachmentTextOutsideLibrary: vi.fn(async (on) => { outside = on; return on; }),
    onSettingsChanged: async () => () => {},
    ...overrides,
  };
  render(<AttachmentText api={api} />);
  return api;
}

test("renders a switch per open library, with names, saved choices and the reading boundary", async () => {
  group({ attachmentTextLibraries: async () => [library, { id: "lib_other_library", name: "Other references", on: true }] });
  expect(await screen.findByRole("heading", { name: "Attachment text" })).toBeTruthy();
  await waitFor(() => expect(screen.getAllByRole("switch")).toHaveLength(3));
  expect(toggle().checked).toBe(false);
  expect(screen.getByRole("switch", { name: `${label} Other references` })).toHaveProperty("checked", true);
  expect(document.body.textContent).toContain("with page numbers, never the files");
  expect(toggle().getAttribute("aria-describedby")).toBe("attachment-text-help");
  expect(document.getElementById("attachment-text-help")?.textContent).toContain("This switch covers connected AI apps");
  expect(document.getElementById("attachment-text-help")?.textContent).toContain("A terminal running ibis does not need this switch");
});

test("toggles only the returned LibraryId and restores the saved choice when a save fails", async () => {
  const api = group();
  await screen.findByRole("switch", { name: `${label} ${library.name}` });
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
  await screen.findByRole("switch", { name: `${label} ${library.name}` });
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
  await screen.findByRole("switch", { name: `${label} ${library.name}` });
  fireEvent.focus(window);
  fireEvent.click(toggle());
  await act(async () => older.resolve([library]));
  expect(toggle().checked).toBe(true);
});

test("no open library still shows the device permission switch", async () => {
  group({ attachmentTextLibraries: async () => [] });
  expect(await screen.findByText("No library is open.")).toBeTruthy();
  expect(screen.getByRole("switch", { name: "Allow attachment text outside the library’s folder" })).toHaveProperty("checked", false);
});

test("a library closed during a pending save is refreshed after the save", async () => {
  const saving = deferred<boolean>();
  let changed!: () => void;
  const api = group({
    attachmentTextLibraries: vi.fn().mockResolvedValueOnce([library]).mockResolvedValue([]),
    setAttachmentText: () => saving.promise,
    onLibraryChanged: async (listener) => { changed = () => listener(testStates[0]!.library); return () => {}; },
  });
  await screen.findByRole("switch", { name: `${label} ${library.name}` });
  fireEvent.click(toggle());
  act(() => changed());
  expect(api.attachmentTextLibraries).toHaveBeenCalledTimes(1);
  await act(async () => saving.resolve(true));
  expect(await screen.findByText("No library is open.")).toBeTruthy();
});

test("outside-library permission is off by default, saves, and restores a refused save", async () => {
  const api = group();
  const outside = await screen.findByRole<HTMLInputElement>("switch", { name: "Allow attachment text outside the library’s folder" });
  expect(outside.checked).toBe(false);
  fireEvent.click(outside);
  await waitFor(() => expect(api.setAttachmentTextOutsideLibrary).toHaveBeenCalledWith(true));
  expect(outside.checked).toBe(true);
  expect(api.setAttachmentText).not.toHaveBeenCalled();
  vi.mocked(api.setAttachmentTextOutsideLibrary).mockRejectedValueOnce(new Error("invented refusal"));
  fireEvent.click(outside);
  await screen.findByRole("alert");
  expect(outside.checked).toBe(true);
});

test.each(["success", "failure"])("a late outside-library save's %s cannot undo a newer choice", async (outcome) => {
  const older = deferred<boolean>();
  group({ setAttachmentTextOutsideLibrary: vi.fn().mockReturnValueOnce(older.promise).mockResolvedValueOnce(false) });
  const outside = await screen.findByRole<HTMLInputElement>("switch", { name: "Allow attachment text outside the library’s folder" });
  fireEvent.click(outside);
  fireEvent.click(outside);
  await act(async () => { if (outcome === "success") older.resolve(true); else older.reject(new Error("late refusal")); });
  expect(outside.checked).toBe(false);
  expect(screen.queryByRole("alert")).toBeNull();
});
