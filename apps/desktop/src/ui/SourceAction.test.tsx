import { act, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { useSourceOpen } from "../features/picker/useSourceOpen";
import { SourceAction, SourceIcon, shortenedLabel } from "./SourceAction";
import type { SourcePresentation } from "../bindings";

const nativeIcon = { mime: "image/png", bytes: [137, 80, 78, 71] };

test("with no snapshot, a Zotero library is named as Zotero before its source is read", () => {
  const library = { zotero: true, name: "Zotero", has_snapshot: false, snapshot_zotero: false, snapshot_name: "earlier.bib", revision: "1", source_generation: "1" } as never;
  const { result } = renderHook(() => useSourceOpen({ librarySource: undefined } as never, library, 0, null, false, () => {}));
  expect(result.current.source).toMatchObject({ kind: "zotero", name: "Zotero" });
});

test("a native source icon is decorative; decode failure retains the named, working action", () => {
  const open = vi.fn();
  render(<SourceAction label="Open Zotero" icon={nativeIcon} open={open} />);
  const button = screen.getByRole("button", { name: "Open Zotero" });
  const image = button.querySelector("img")!;
  expect(image.alt).toBe("");
  expect(image.src).toBe("data:image/png;base64,iVBORw==");
  fireEvent.error(image);
  expect(button.querySelector("img")).toBeNull();
  expect(button.querySelector("svg path")).not.toBeNull();
  fireEvent.click(button);
  expect(open).toHaveBeenCalledOnce();
});

test.each([null, { mime: "image/svg+xml", bytes: [60, 115, 118, 103] }, { mime: "image/png", bytes: [256] }])(
  "missing or refused artwork shows the books fallback", (icon) => {
    const { container } = render(<SourceIcon icon={icon} />);
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("svg path")).not.toBeNull();
  });

function deferred<T>() {
  let resolve!: (value: T) => void, reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

test("failed icon I/O, delayed A after B and close/reopen keep the current visible source", async () => {
  const replies = Array.from({ length: 4 }, () => deferred<SourcePresentation>());
  const read = vi.fn().mockImplementationOnce(() => replies[0]!.promise).mockImplementationOnce(() => replies[1]!.promise)
    .mockImplementationOnce(() => replies[2]!.promise).mockImplementationOnce(() => replies[3]!.promise);
  const library = { revision: "1", source_generation: "1", zotero: true, name: "Zotero" } as never;
  function Presentation({ visit, item }: { visit: number; item: string }) {
    const { source } = useSourceOpen({ librarySource: read } as never, library, visit, item, false, () => {});
    return <SourceAction label="Open Zotero" icon={source.icon} open={() => {}} />;
  }
  const view = render(<Presentation visit={0} item="A" />);
  view.rerender(<Presentation visit={0} item="B" />);
  await act(async () => replies[1]!.resolve({ kind: "zotero", name: "Zotero", icon: nativeIcon, status_icon: null }));
  expect(screen.getByRole("button", { name: "Open Zotero" }).querySelector("img")).not.toBeNull();
  await act(async () => replies[0]!.resolve({ kind: "zotero", name: "Zotero", icon: null, status_icon: null }));
  expect(screen.getByRole("button", { name: "Open Zotero" }).querySelector("img")).not.toBeNull();
  view.rerender(<Presentation visit={1} item="A" />);
  await act(async () => replies[2]!.reject(new Error("invented icon read failure")));
  expect(screen.getByRole("button", { name: "Open Zotero" }).querySelector("img")).toBeNull();
  view.unmount();
  render(<Presentation visit={2} item="B" />);
  expect(screen.getByRole("button", { name: "Open Zotero" }).querySelector("img")).toBeNull();
  await act(async () => replies[3]!.resolve({ kind: "zotero", name: "Zotero", icon: nativeIcon, status_icon: null }));
  expect(screen.getByRole("button", { name: "Open Zotero" }).querySelector("img")).not.toBeNull();
});

afterEach(() => { vi.useRealTimers(); });

test("long names shorten in the middle, keep their extension and name the whole file", () => {
  const label = `Find ${"invented-thesis-references-".repeat(3)}final.bib`;
  expect(shortenedLabel(label).length).toBe(44);
  expect(shortenedLabel(label).endsWith("….bib")).toBe(true);
  expect(shortenedLabel(label).startsWith("Find invented")).toBe(true);
  expect(shortenedLabel("Find invented.bib")).toBe("Find invented.bib");
  render(<SourceAction label={label} icon={null} file open={() => {}} />);
  const action = screen.getByRole("button", { name: label });
  expect(action.getAttribute("title")).toBe(label);
  expect(action.textContent).toBe(shortenedLabel(label));
});

test("shortening never splits an emoji or a combining accent", () => {
  const scientist = "👩‍🔬", accent = "e\u0301";
  expect(shortenedLabel(`${"a".repeat(31)}${scientist}${"z".repeat(20)}.bib`)).toContain(scientist);
  expect(shortenedLabel(`${"a".repeat(38)}${accent}${"z".repeat(20)}.bib`)).toContain(accent);
});

test("a short name has no tooltip, and busy replaces the icon with a spinner", () => {
  const { container } = render(<SourceAction label="Open invented.bib" icon={null} file busy open={() => {}} />);
  const action = screen.getByRole("button", { name: "Open invented.bib" });
  expect(action.getAttribute("title")).toBeNull();
  expect(action.getAttribute("aria-busy")).toBe("true");
  expect(container.querySelector(".source-spinner")).not.toBeNull();
  expect(container.querySelector(".source-icon")).toBeNull();
});

function sourceOpen(open: () => Promise<unknown>) {
  const library = { revision: "1", name: "invented.bib" } as never;
  return renderHook(() => useSourceOpen({ openLibrarySource: open, librarySource: undefined } as never, library, 0, null, false, () => {}));
}

test("a slow open is busy only after 400 ms, and a fast one never is", async () => {
  vi.useFakeTimers();
  let finish!: () => void;
  const slow = sourceOpen(() => new Promise<void>((resolve) => { finish = resolve; }));
  act(() => slow.result.current.run());
  act(() => { vi.advanceTimersByTime(399); });
  expect(slow.result.current.busy).toBe(false);
  act(() => { vi.advanceTimersByTime(1); });
  expect(slow.result.current.busy).toBe(true);
  await act(async () => { finish(); });
  expect(slow.result.current.busy).toBe(false);
  const fast = sourceOpen(() => Promise.resolve());
  await act(async () => { fast.result.current.run(); });
  act(() => { vi.advanceTimersByTime(1000); });
  expect(fast.result.current.busy).toBe(false);
});

test("a failed open stops being busy, and a late first result cannot clear a newer open", async () => {
  vi.useFakeTimers();
  const finish: (() => void)[] = [];
  let reject!: (reason: unknown) => void;
  const hook = sourceOpen(() => new Promise<void>((resolve) => { finish.push(resolve); }));
  act(() => hook.result.current.run());
  act(() => hook.result.current.run());
  act(() => { vi.advanceTimersByTime(400); });
  expect(hook.result.current.busy).toBe(true);
  await act(async () => { finish[0]!(); });
  expect(hook.result.current.busy).toBe(true);
  await act(async () => { finish[1]!(); });
  expect(hook.result.current.busy).toBe(false);
  const failed = sourceOpen(() => new Promise((_, no) => { reject = no; }));
  act(() => failed.result.current.run());
  act(() => { vi.advanceTimersByTime(400); });
  await act(async () => { reject(new Error("invented refusal")); });
  expect(failed.result.current.busy).toBe(false);
  expect(failed.result.current.failure).not.toBeNull();
});
