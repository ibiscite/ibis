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
  expect(screen.getByText(/window places,? stay on this one/i)).toBeTruthy();
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
    "In Word, LibreOffice and other documents: APA → IEEE",
    "Library: Default → thesis.bib",
    "Rows: Comfortable → Compact",
    "Text size: Default → On",
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

test("settings that changed after the preview refuse the restore and show why", async () => {
  const api = fakeApi({ restoreSettingsBackup: vi.fn().mockRejectedValueOnce({ code: "changed", message: { id: "backup-problem-changed", args: {} }, recovery: { action: "choose-backup" }, focus: "recovery-action" }) });
  render(<BackupRows api={api} />);
  fireEvent.click(screen.getByRole("button", { name: "Restore…" }));
  await act(async () => undefined);
  fireEvent.click(screen.getByRole("button", { name: "Restore and Restart" }));
  await act(async () => undefined);
  expect(screen.getByRole("alert").textContent).toMatch(/Settings changed after the backup was shown/);
  expect(screen.queryByRole("listitem")).toBeNull();
  expect(screen.getByRole("button", { name: "Restore…" }).getAttribute("aria-disabled")).not.toBe("true");
});
