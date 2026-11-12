import { afterEach, expect, test, vi } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import type { LibraryStatus } from "./api";
import { testStates } from "./test-fixtures";
import { backend, chooseFromLibrary, resetApp, result } from "./testing/app";
import { ZOTERO_CHECK_EVERY } from "./features/picker/EmptyState";

afterEach(resetApp);

test("a failed Zotero switch recovers over retained file results without focus changes, pauses hidden and resumes on reopen", async () => {
  let status: LibraryStatus = {
    ...testStates.find((state) => state.id === "no-results")!.library,
    state: "loaded", has_snapshot: true, references: 1, name: "refs.bib", snapshot_name: "refs.bib",
    revision: "1", source_generation: "1", source_is_requested: true, zotero: false,
    warnings: 0, error: null, problem: null, note: null,
  };
  let ready = false;
  const check = vi.fn(async () => {
    if (ready) {
      status = { ...status, state: "loaded", revision: "2", source_generation: "2", source_is_requested: true,
        name: "Zotero", snapshot_name: "Zotero", snapshot_zotero: true, zotero_libraries: ["user"], error: null, problem: null };
      await emit("library:changed", status);
    }
    return ready ? "available" : "access_off";
  });
  backend({
    library_status: () => status,
    search: () => ({ revision: status.revision, results: [
      { ...result("invented", status.revision === "2" ? "Zotero atlas" : "Retained file atlas"), revision: status.revision },
    ] }),
    use_zotero: async () => {
      status = { ...status, state: "error", zotero: true, source_is_requested: false,
        name: "Zotero", snapshot_zotero: false, problem: "zotero_access_off", error: "Zotero is not sharing your library." };
      await emit("library:changed", status);
    },
    check_zotero_sharing: check,
  });
  render(<App />);
  await screen.findByText("Retained file atlas");
  await chooseFromLibrary("Use Zotero");
  expect(screen.getByText("Retained file atlas")).toBeTruthy();
  await waitFor(() => expect(check).toHaveBeenCalled(), { timeout: ZOTERO_CHECK_EVERY * 2 });
  ready = true;
  await screen.findByText("Zotero atlas", {}, { timeout: ZOTERO_CHECK_EVERY * 2 });
  expect(screen.queryByText("Retained file atlas")).toBeNull();
  // Reproduce a second failure, then make Zotero ready while the picker is hidden.
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  ready = false;
  status = { ...status, state: "error", problem: "zotero_access_off", error: "Zotero is not sharing your library." };
  await act(async () => { await emit("library:changed", status); });
  await act(async () => { await emit("picker:hidden"); });
  const hiddenChecks = check.mock.calls.length;
  ready = true;
  await act(async () => { await vi.advanceTimersByTimeAsync(ZOTERO_CHECK_EVERY * 3); });
  expect(check).toHaveBeenCalledTimes(hiddenChecks);
  await act(async () => { await emit("picker:shown", 2); });
  await act(async () => { await vi.advanceTimersByTimeAsync(ZOTERO_CHECK_EVERY); });
  expect(status.error).toBeNull();
  expect(screen.getByText("Zotero atlas")).toBeTruthy();
}, 15_000);
