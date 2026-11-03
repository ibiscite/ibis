import { act, renderHook, waitFor } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import type { PickerApi } from "../../App";
import type { LibraryStatus } from "../../api";
import { useLibraryStatus } from "./useLibraryStatus";

function status(state: LibraryStatus["state"], revision = "1"): LibraryStatus {
  return { source_generation: "1", source_is_requested: true, revision, state, has_snapshot: true, name: "synthetic.bib", snapshot_name: "synthetic.bib", references: 1,
    zotero: false, zotero_libraries: [], snapshot_zotero: false, source_counts: {}, warnings: 0, error: null } as unknown as LibraryStatus;
}

function setup(first: LibraryStatus) {
  let changed!: (status: LibraryStatus) => void;
  const api = {
    libraryStatus: vi.fn(async () => first),
    onLibraryChanged: vi.fn(async (handler: typeof changed) => { changed = handler; return () => undefined; }),
    onPickerHidden: vi.fn(async () => () => undefined), onPickerShown: vi.fn(async () => () => undefined),
  } as unknown as PickerApi;
  const hook = renderHook(() => useLibraryStatus(api, vi.fn(), vi.fn()));
  return { ...hook, change: (next: LibraryStatus) => act(async () => changed(next)) };
}

test("a reload changes only the live status; everything else waits for its end", async () => {
  const loaded = status("loaded");
  const { result, change } = setup(loaded);
  await waitFor(() => expect(result.current.library).toBe(loaded));
  const version = result.current.libraryVersion;
  const reloading = status("reloading");
  await change(reloading);
  expect(result.current.library).toBe(reloading);
  expect(result.current.settledLibrary).toBe(loaded);
  expect(result.current.libraryVersion).toBe(version);

  const fresh = status("loaded", "2");
  await change(fresh);
  expect(result.current.settledLibrary).toBe(fresh);
  expect(result.current.libraryVersion).toBe(version + 1);
});

test("a failed reload settles too, so the kept library is searched again", async () => {
  const { result, change } = setup(status("loaded"));
  await waitFor(() => expect(result.current.settledLibrary).toBeTruthy());
  const version = result.current.libraryVersion;
  await change(status("reloading"));
  const failed = status("error");
  await change(failed);
  expect(result.current.settledLibrary).toBe(failed);
  expect(result.current.libraryVersion).toBe(version + 1);
});

test("a later reload's end wins whichever order the reloads began", async () => {
  const { result, change } = setup(status("loaded"));
  await waitFor(() => expect(result.current.settledLibrary).toBeTruthy());
  await change(status("reloading", "2"));
  await change(status("reloading", "3"));
  const newest = status("loaded", "3");
  await change(newest);
  expect(result.current.settledLibrary).toBe(newest);
});

test("opening on a reload still has a library to show", async () => {
  const reloading = status("reloading");
  const { result } = setup(reloading);
  await waitFor(() => expect(result.current.settledLibrary).toBe(reloading));
});
