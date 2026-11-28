import { act, cleanup, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { PasteGuide, usePastePermission } from "./PasteGuide";

afterEach(() => { cleanup(); vi.useRealTimers(); vi.restoreAllMocks(); });
const denied = { pastes: true, needed: true, granted: false, list: "Accessibility" };

test.each(["Accessibility", "Device Control and Data Access"])("the %s drag is a fallback beside automatic listing, with no claimed grant", async (list) => {
  const onOpen = vi.fn();
  const onDrag = vi.fn(async () => undefined);
  const { rerender } = render(<PasteGuide name="Ibis Nightly" permission={{ ...denied, list }} onOpen={onOpen} onDrag={onDrag} />);
  const open = screen.getByRole("button", { name: "Open System Settings" });
  const drag = screen.getByRole("button", { name: `Drag Ibis Nightly to ${list}` });
  expect(open.parentElement).toBe(drag.parentElement);
  expect(open.compareDocumentPosition(drag) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(screen.getByText("If Ibis is not in the list, drag it there.")).toBeTruthy();
  expect(drag.querySelector("img")?.getAttribute("src")).toContain("128x128.png");
  expect(onOpen).not.toHaveBeenCalled();
  expect(onDrag).not.toHaveBeenCalled();
  expect(screen.getByText(/Accessibility access to check which window has the keyboard/)).toBeTruthy();
  expect(screen.getByText("Return to your document and open Ibis again. Press Enter to try again.")).toBeTruthy();
  fireEvent.click(open);
  expect(onOpen).toHaveBeenCalledOnce();
  expect(fireEvent.dragStart(drag)).toBe(false); // suppress the webview's image/HTML drag
  await act(async () => undefined);
  expect(onDrag).toHaveBeenCalledOnce();
  expect(onDrag.mock.calls[0]).toEqual([]); // no file path from the webview
  expect(onOpen).toHaveBeenCalledOnce();
  expect(screen.getByRole("status").textContent).toBe("Waiting for your OK…");
  rerender(<PasteGuide name="Ibis Nightly" permission={{ ...denied, list, granted: true }} onOpen={onOpen} onDrag={onDrag} />);
  expect(screen.getByRole("status").textContent).toBe("Pasting is on.");
  expect(screen.queryByRole("button")).toBeNull();
  expect(screen.queryByText("If Ibis is not in the list, drag it there.")).toBeNull();
});

test("an unavailable drag explains recovery, and retry clears the failure without claiming a grant", async () => {
  const onDrag = vi.fn().mockRejectedValueOnce(new Error("private synthetic detail")).mockResolvedValueOnce(undefined);
  render(<PasteGuide name="Ibis" permission={denied} onOpen={vi.fn()} onDrag={onDrag} />);
  const drag = screen.getByRole("button", { name: "Drag Ibis to Accessibility" });
  fireEvent.dragStart(drag);
  expect((await screen.findByRole("alert")).textContent).toBe("Couldn’t drag this copy of Ibis. Use the + button in System Settings to add it.");
  expect(screen.queryByText(/private synthetic detail/)).toBeNull();
  expect(screen.queryByText("Pasting is on.")).toBeNull();
  await act(async () => { fireEvent.dragStart(drag); });
  expect(screen.queryByRole("alert")).toBeNull();
  expect(screen.queryByText("Pasting is on.")).toBeNull();
});

test("a failed read does not claim permission was granted", async () => {
  const api = { pastePermission: vi.fn().mockResolvedValueOnce(denied).mockRejectedValue(new Error("synthetic failure")) };
  const { result } = renderHook(() => usePastePermission(api, true));
  await act(async () => { await Promise.resolve(); });
  await act(async () => { window.dispatchEvent(new Event("focus")); });
  expect(result.current?.granted).toBe(false);
});
