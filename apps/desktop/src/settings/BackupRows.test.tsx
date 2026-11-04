import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { RestorePreview } from "../bindings";
import { type BackupApi, BackupRows } from "./BackupRows";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

const preview = (ticket: number, changes: RestorePreview["changes"] = [
  { setting: "output", from: "apa", to: "ieee" },
  { setting: "library", from: null, to: "thesis.bib" },
  { setting: "density", from: "comfortable", to: "compact" },
  { setting: "larger_text", from: null, to: "on" },
  { setting: "shortcut.windows", from: null, to: "Alt+Space" },
]): RestorePreview => ({ ticket, made_on: "windows", made_by: "0.2.8", changes, notes: ["library-not-found"] });

function deferred<T>() {
  let resolve!: (value: T) => void, reject!: (error: unknown) => void;
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}

function fakeApi(overrides: Partial<BackupApi> = {}): BackupApi {
  return {
    backUpSettings: vi.fn(async () => "Ibis settings.json"),
    chooseSettingsBackup: vi.fn(async () => preview(1)),
    restoreSettingsBackup: vi.fn(async () => null),
    cancelSettingsRestore: vi.fn(async () => null),
    ...overrides,
  };
}

test("a backup is saved and named, and a cancelled save says nothing", async () => {
  const api = fakeApi({ backUpSettings: vi.fn().mockResolvedValueOnce("Ibis settings.json").mockResolvedValueOnce(null) });
  render(<BackupRows api={api} />);
  expect(screen.getByText(/Paste approvals and window places stay on this one/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Back Up…" }));
  await act(async () => undefined);
  expect(screen.getByRole("status").textContent).toBe("Saved to Ibis settings.json.");
  fireEvent.click(screen.getByRole("button", { name: "Back Up…" }));
  await act(async () => undefined);
  expect(screen.queryByRole("status")).toBeNull();
  expect(screen.queryByRole("alert")).toBeNull();
});

test("a restore shows every change in words first, then restores that preview and restarts", async () => {
  const api = fakeApi();
  render(<BackupRows api={api} />);
  fireEvent.click(screen.getByRole("button", { name: "Restore…" }));
  await act(async () => undefined);
  expect(screen.getByText("Backup from Windows, made by Ibis 0.2.8")).toBeTruthy();
  const items = screen.getAllByRole("listitem").map((item) => item.textContent);
  expect(items).toEqual([
    "Default format: APA → IEEE",
    "Library: Default → thesis.bib",
    "Rows: Comfortable → Compact",
    "Larger text: Default → On",
    "Opening shortcut on Windows: Default → Alt+Space",
  ]);
  expect(screen.getByText(/bibliography is not on this computer/)).toBeTruthy();
  expect(api.restoreSettingsBackup).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Restore and Restart" }));
  await act(async () => undefined);
  expect(api.restoreSettingsBackup).toHaveBeenCalledWith(1);
  expect(screen.getByRole("status").textContent).toMatch(/Restarting Ibis/);
  expect(screen.getByRole("button", { name: "Back Up…" }).hasAttribute("disabled")
    || screen.getByRole("button", { name: "Back Up…" }).getAttribute("aria-disabled") === "true").toBe(true);
});

test("cancelling forgets the preview, and a backup with no changes offers nothing to restore", async () => {
  const api = fakeApi({ chooseSettingsBackup: vi.fn().mockResolvedValueOnce(preview(4)).mockResolvedValueOnce(preview(5, [])) });
  render(<BackupRows api={api} />);
  fireEvent.click(screen.getByRole("button", { name: "Restore…" }));
  await act(async () => undefined);
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(api.cancelSettingsRestore).toHaveBeenCalledWith(4);
  expect(screen.queryByRole("listitem")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Restore…" }));
  await act(async () => undefined);
  expect(screen.getByText("This computer already has these settings.")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Restore and Restart" })).toBeNull();
});

test.each([
  ["not-downloaded", /isn’t downloaded to this computer yet/],
  ["settings-file", /Ibis’s own settings file, not a backup/],
  ["newer", /newer version of Ibis/],
  ["invalid", /backup is damaged\. Nothing changed/],
  [new Error("private synthetic detail"), /couldn’t save\. Nothing changed/],
])("a refused backup (%s) is explained and nothing is restored", async (refusal, words) => {
  const api = fakeApi({ chooseSettingsBackup: vi.fn().mockRejectedValueOnce(refusal) });
  render(<BackupRows api={api} />);
  fireEvent.click(screen.getByRole("button", { name: "Restore…" }));
  await act(async () => undefined);
  expect(screen.getByRole("alert").textContent).toMatch(words);
  expect(screen.queryByText(/private/)).toBeNull();
  expect(api.restoreSettingsBackup).not.toHaveBeenCalled();
});

test("settings that changed after the preview refuse the restore and show why", async () => {
  const api = fakeApi({ restoreSettingsBackup: vi.fn().mockRejectedValueOnce("changed") });
  render(<BackupRows api={api} />);
  fireEvent.click(screen.getByRole("button", { name: "Restore…" }));
  await act(async () => undefined);
  fireEvent.click(screen.getByRole("button", { name: "Restore and Restart" }));
  await act(async () => undefined);
  expect(screen.getByRole("alert").textContent).toMatch(/Settings changed after the backup was shown/);
  expect(screen.queryByRole("listitem")).toBeNull();
  expect(screen.getByRole("button", { name: "Restore…" }).getAttribute("aria-disabled")).not.toBe("true");
});

test("a preview that arrives after Settings closed is forgotten, and closing forgets the shown one", async () => {
  const late = deferred<RestorePreview | null>();
  const api = fakeApi({ chooseSettingsBackup: vi.fn().mockReturnValueOnce(late.promise).mockResolvedValueOnce(preview(9)) });
  const first = render(<BackupRows api={api} />);
  fireEvent.click(screen.getByRole("button", { name: "Restore…" }));
  first.unmount();
  await act(async () => { late.resolve(preview(8)); });
  expect(api.cancelSettingsRestore).toHaveBeenCalledWith(8);
  const second = render(<BackupRows api={api} />);
  expect(screen.queryByRole("listitem")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Restore…" }));
  await act(async () => undefined);
  second.unmount();
  expect(api.cancelSettingsRestore).toHaveBeenCalledWith(9);
  expect(api.restoreSettingsBackup).not.toHaveBeenCalled();
});

test("a damaged settings file can be repaired from a backup with the same settings", async () => {
  const api = fakeApi({ chooseSettingsBackup: vi.fn(async () => ({ ...preview(6, []), notes: ["repairs-settings" as const] })) });
  render(<BackupRows api={api} />);
  fireEvent.click(screen.getByRole("button", { name: "Restore…" }));
  await act(async () => undefined);
  expect(screen.getByText(/settings file on this computer is damaged or missing/)).toBeTruthy();
  expect(screen.queryByText("This computer already has these settings.")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Restore and Restart" }));
  await act(async () => undefined);
  expect(api.restoreSettingsBackup).toHaveBeenCalledWith(6);
});
