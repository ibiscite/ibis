import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { FilePreview } from "../../bindings";
import type { LibraryStatus } from "../../api";
import type { CitationReference } from "./choice";
import { pickerText } from "../../messages";
import { useCitationOutput } from "./useCitationOutput";

afterEach(cleanup);

type Options = Parameters<typeof useCitationOutput>[0];
type Preview = NonNullable<Options["previewFileCitation"]>;

function library(revision = "1"): LibraryStatus {
  return {
    revision, state: "loaded", has_snapshot: true,
    name: "synthetic.bib", snapshot_name: "synthetic.bib", references: 1,
    zotero: false, zotero_libraries: [], snapshot_zotero: false,
    source_counts: { "file:synthetic": 1 }, warnings: 0, error: null,
    problem: null, note: null, settings_notice: null, stale: false,
    updated_at: null, checked_at: null,
  };
}

function reference(revision = "1", key = "synthetic"): CitationReference {
  return {
    revision, id: { library: "file:synthetic", key }, cite: `synthetic-cite-${revision}-${key}`,
    source: "file:synthetic", key, title: "Synthetic reference", authors: "Example",
    year: 2026, matched: ["title"], partial: false, first_author: "Example", author_count: 1,
    container: null, item_type: "book",
  };
}

function preview(text: string): FilePreview {
  return { text, html: null, warnings: [], ticket: "synthetic-ticket", content_hash: "synthetic-hash" };
}

function options(overrides: Partial<Options>): Options {
  return {
    chosen: [reference()], library: library(), libraryVersion: 0,
    format: "pandoc", style: null, showPastePreview: true,
    previewFileCitation: vi.fn<Preview>().mockResolvedValue(preview("[@synthetic]")),
    refreshSnapshot: vi.fn(), ...overrides,
  };
}

const conflictMessage = "These references have conflicting citation keys. Resolve them in your library or choose a formatted style.";

test("replacing, clearing and unmounting a selection aborts obsolete engine previews", () => {
  const signals: AbortSignal[] = [];
  const previewFileCitation = vi.fn<Preview>((_items, _format, _style, _revision, signal) => {
    if (signal) signals.push(signal);
    return new Promise(() => {});
  });
  const initial = options({ previewFileCitation });
  const { rerender, unmount } = renderHook(useCitationOutput, { initialProps: initial });
  expect(signals).toHaveLength(1);
  rerender({ ...initial, chosen: [reference("1", "replacement")] });
  expect(signals[0]?.aborted).toBe(true);
  expect(signals[1]?.aborted).toBe(false);
  rerender({ ...initial, chosen: [] });
  expect(signals[1]?.aborted).toBe(true);
  rerender(initial);
  expect(signals[2]?.aborted).toBe(false);
  unmount();
  expect(signals[2]?.aborted).toBe(true);
});

test.each([
  { edit: "page", details: { locator: { label: "page", value: "42–45" }, form: "normal" } },
  { edit: "cleared page", details: { locator: null, form: "normal" } },
  { edit: "narrative form", details: { locator: { label: "page", value: "12" }, form: "narrative" } },
  { edit: "suppressed author", details: { locator: { label: "page", value: "12" }, form: "author-omitted" } },
] as const)("a $edit edit aborts the old preview and keeps the new receipt after a late result", async ({ details }) => {
  const pending: ((value: FilePreview) => void)[] = [];
  const previewFileCitation = vi.fn<Preview>(() => new Promise((resolve) => pending.push(resolve)));
  const chosen: CitationReference = { ...reference(), locator: { label: "page", value: "12" }, form: "normal" };
  const refreshSnapshot = vi.fn();
  const initial = options({ chosen: [chosen], previewFileCitation, refreshSnapshot });
  const { result, rerender, unmount } = renderHook(useCitationOutput, { initialProps: initial });
  const oldSignal = previewFileCitation.mock.calls[0]?.[4];
  expect(oldSignal).toBeInstanceOf(AbortSignal);
  expect(oldSignal?.aborted).toBe(false);

  rerender({ ...initial, chosen: [{ ...chosen, ...details }] });
  expect(oldSignal?.aborted).toBe(true);
  expect(result.current.shown).toBeNull();
  const signal = previewFileCitation.mock.calls[1]?.[4];
  expect(signal).toBeInstanceOf(AbortSignal);
  expect(signal?.aborted).toBe(false);
  expect(previewFileCitation).toHaveBeenLastCalledWith(
    [{ id: chosen.id, cite: chosen.cite, ...details }], "pandoc", null, "1", signal,
  );

  await act(async () => { pending[1]?.(preview("edited citation")); });
  expect(result.current.shown).toMatchObject({ text: "edited citation", fileTicket: "synthetic-ticket" });
  await act(async () => { pending[0]?.(preview("obsolete citation")); });
  expect(result.current.shown).toMatchObject({ text: "edited citation", fileTicket: "synthetic-ticket" });
  expect(refreshSnapshot).not.toHaveBeenCalled();
  unmount();
});

test("a failed native preview keeps its actionable explanation without refreshing the snapshot", async () => {
  const previewFileCitation = vi.fn<Preview>().mockRejectedValue({ code: "failed", message: conflictMessage });
  const refreshSnapshot = vi.fn();
  const { result } = renderHook(useCitationOutput, {
    initialProps: options({ previewFileCitation, refreshSnapshot }),
  });

  await waitFor(() => expect(result.current.shown).toBe("failed"));
  expect(result.current.formatted?.problem).toBe(conflictMessage);
  expect(result.current.previewProblem).toBe(conflictMessage);
  expect(result.current.displayedOutput).toBe("failed");
  expect(refreshSnapshot).not.toHaveBeenCalled();
});

test("a shared citation key keeps its existing explanation ahead of a supplied message", async () => {
  const previewFileCitation = vi.fn<Preview>().mockRejectedValue({
    code: "shared_citation_key", key: "synthetic", message: conflictMessage,
  });
  const { result } = renderHook(useCitationOutput, {
    initialProps: options({ previewFileCitation }),
  });

  await waitFor(() => expect(result.current.shown).toBe("failed"));
  expect(result.current.previewProblem).toBe(pickerText.sharedKey("synthetic"));
});

test("a stale file preview waits for a changed source instead of retrying each unchanged status read", async () => {
  // Unexpected retries stay pending so the regression fails on a call count,
  // rather than running an unbounded rejection/status-refresh loop.
  const previewFileCitation = vi.fn<Preview>()
    .mockRejectedValueOnce({ code: "snapshot_changed" })
    .mockReturnValue(new Promise<FilePreview>(() => undefined));
  const refreshSnapshot = vi.fn();
  const initial = options({ previewFileCitation, refreshSnapshot });
  const { result, rerender } = renderHook(useCitationOutput, { initialProps: initial });
  await waitFor(() => expect(result.current.shown).toBe("stale"));
  expect(refreshSnapshot).toHaveBeenCalledTimes(1);

  for (const libraryVersion of [1, 2]) {
    await act(async () => {
      rerender({ ...initial, library: { ...library() }, libraryVersion });
    });
    expect(previewFileCitation).toHaveBeenCalledTimes(1);
    expect(refreshSnapshot).toHaveBeenCalledTimes(1);
    expect(result.current.shown).toBe("stale");
  }
});

test("a stale file preview recovers when a refreshed search has a new source revision", async () => {
  const previewFileCitation = vi.fn<Preview>()
    .mockRejectedValueOnce({ code: "snapshot_changed" })
    .mockResolvedValue(preview("[@fresh]"));
  const refreshSnapshot = vi.fn();
  const initial = options({ previewFileCitation, refreshSnapshot });
  const { result, rerender } = renderHook(useCitationOutput, { initialProps: initial });
  await waitFor(() => expect(result.current.shown).toBe("stale"));

  rerender({ ...initial, library: library("2"), chosen: [reference("2")], libraryVersion: 1 });
  await waitFor(() => expect(result.current.shown).toMatchObject({ text: "[@fresh]", fileTicket: "synthetic-ticket" }));
  expect(previewFileCitation).toHaveBeenCalledTimes(2);
  expect(refreshSnapshot).toHaveBeenCalledTimes(1);
});

test.each([
  { code: "snapshot_changed" },
  { code: "failed", message: conflictMessage },
])("a replaced file preview cannot refresh status or replace newer output after a late $code rejection", async (error) => {
  let rejectOld: (reason: unknown) => void = () => undefined;
  const old = new Promise<FilePreview>((_resolve, reject) => { rejectOld = reject; });
  const previewFileCitation = vi.fn<Preview>()
    .mockReturnValueOnce(old)
    .mockResolvedValue(preview("[@new-choice]"));
  const refreshSnapshot = vi.fn();
  const initial = options({ previewFileCitation, refreshSnapshot });
  const { result, rerender } = renderHook(useCitationOutput, { initialProps: initial });
  rerender({ ...initial, chosen: [reference("1", "new-choice")] });
  await waitFor(() => expect(result.current.shown).toMatchObject({ text: "[@new-choice]" }));

  await act(async () => { rejectOld(error); });
  expect(refreshSnapshot).not.toHaveBeenCalled();
  expect(result.current.shown).toMatchObject({ text: "[@new-choice]" });
  expect(result.current.previewProblem).toBeUndefined();
});
