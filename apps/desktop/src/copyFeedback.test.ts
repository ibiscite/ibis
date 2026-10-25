import { expect, test } from "vitest";
import { afterCopy, DRAG_DISTANCE, leaveNotice, noticePlatform, renderNotice, rowClickAction, type RowClick } from "./copyFeedback";
import noticeCapability from "../src-tauri/capabilities/notice.json";

test("only a clean copy from an unpinned picker hides it and says Copied", () => {
  expect(afterCopy({ text: "[@a]", warnings: [] }, false)).toBe("hide-and-notify");
  expect(afterCopy({ text: "[@a]", warnings: [] }, true)).toBe("stay");
  expect(afterCopy({ text: "a", warnings: ["bare keys cannot carry page numbers"] }, false)).toBe("stay");
  expect(afterCopy({ text: "a", warnings: ["w"] }, true)).toBe("stay");
});

const click: RowClick = { shiftKey: false, ctrlKey: false, metaKey: false, altKey: false, detail: 1, moved: 0, onControl: false, selectedText: false };

test("C5 single-click selects without copying; double-click copies", () => {
  expect(rowClickAction(click)).toBe("select");
  expect(rowClickAction({ ...click, detail: 2 })).toBe("copy");
});

test("single clicks select, modifiers toggle, and drags or controls do nothing", () => {
  expect(rowClickAction(click)).toBe("select");
  // Screen readers and keyboards activate with a click of detail 0.
  expect(rowClickAction({ ...click, detail: 0 })).toBe("select");
  expect(rowClickAction({ ...click, shiftKey: true })).toBe("toggle");
  expect(rowClickAction({ ...click, moved: DRAG_DISTANCE })).toBe("select");
  expect(rowClickAction({ ...click, moved: DRAG_DISTANCE + 0.5 })).toBe("none");
  expect(rowClickAction({ ...click, shiftKey: true, moved: 40 })).toBe("none");
  expect(rowClickAction({ ...click, onControl: true })).toBe("none");
  expect(rowClickAction({ ...click, selectedText: true })).toBe("none");
  expect(rowClickAction({ ...click, detail: 2 })).toBe("copy");
  expect(rowClickAction({ ...click, ctrlKey: true })).toBe("toggle");
  expect(rowClickAction({ ...click, metaKey: true }, true)).toBe("toggle");
  expect(rowClickAction({ ...click, ctrlKey: true }, true)).toBe("none");
  for (const modifier of ["metaKey", "altKey"] as const) {
    expect(rowClickAction({ ...click, [modifier]: true })).toBe("none");
  }
});

test("each platform gets its own notice", () => {
  expect(noticePlatform("MacIntel")).toBe("macos");
  expect(noticePlatform("Win32")).toBe("windows");
  expect(noticePlatform("Linux x86_64")).toBe("linux");
  expect(noticePlatform("")).toBe("linux");
});

test("the notice is a polite status that shows text as text and replays when shown again", () => {
  const root = document.createElement("div");
  const show = renderNotice(root, "macos");
  const notice = root.querySelector(".copied-notice")!;
  expect(notice.getAttribute("role")).toBe("status");
  expect(notice.getAttribute("aria-live")).toBe("polite");
  // The live region exists, empty, before its text arrives.
  expect(notice.textContent).toBe("");
  show("<img src=x onerror=alert(1)> [@a]");
  expect(notice.querySelector("img")).toBeNull();
  expect(notice.querySelector("code")?.textContent).toBe("<img src=x onerror=alert(1)> [@a]");
  expect(notice.textContent).toContain("Copied");
  expect(notice.classList.contains("showing")).toBe(true);
  show("[@b]");
  expect(notice.querySelector("code")?.textContent).toBe("[@b]");
  expect(notice.querySelectorAll("code")).toHaveLength(1);
  const windows = document.createElement("div");
  renderNotice(windows, "windows")("[@a]");
  expect(windows.textContent).toContain("Citation copied. Press Ctrl+V to paste.");
});

test("the app's timer fades the notice out, and showing it again brings it back", () => {
  const root = document.createElement("div");
  const show = renderNotice(root, "linux");
  const notice = root.querySelector(".copied-notice")!;
  show("[@a]");
  leaveNotice(root);
  expect(notice.classList.contains("leaving")).toBe(true);
  show("[@b]");
  expect(notice.classList.contains("leaving")).toBe(false);
  expect(notice.classList.contains("showing")).toBe(true);
});

test("the notice window grants only copied text reading and event subscriptions", () => {
  expect(noticeCapability.windows).toEqual(["notice"]);
  expect([...noticeCapability.permissions].sort()).toEqual(["allow-copied-notice", "core:event:allow-listen", "core:event:allow-unlisten"]);
});
