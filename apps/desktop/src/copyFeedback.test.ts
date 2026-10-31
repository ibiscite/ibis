import { expect, test } from "vitest";
import type { NoticeMessage } from "./api";
import { afterCopy, DRAG_DISTANCE, leaveNotice, noticePlatform, noticeTitle, renderNotice, rowClickAction, type RowClick } from "./copyFeedback";

const copied = (text: string, reason: string | null = null): NoticeMessage => ({ delivered: { outcome: "copied", reason }, text });
import noticeCapability from "../src-tauri/capabilities/notice.json";

test.each([["macos", "⌘V"], ["windows", "Ctrl+V"], ["linux", "Ctrl+V"]] as const)("plain-copy receipts always use %s's paste key, including after reopening", (platform, key) => {
  const root = document.createElement("div"), show = renderNotice(root, platform);
  for (let copy = 0; copy < 4; copy++) {
    show(copied(`[@invented${copy}]`));
    expect(root.querySelector("strong")?.textContent).toBe(`Copied. Press ${key} to paste`);
  }
  renderNotice(root, platform)(copied("[@reopened]"));
  expect(root.querySelector("strong")?.textContent).toBe(`Copied. Press ${key} to paste`);
});

test("the paste key never replaces a refusal, Linux's explanation or a pasted result", () => {
  const root = document.createElement("div"), show = renderNotice(root, "linux");
  for (const [reason, title] of [["terminal", "Copied. Not pasted into a terminal"],
    ["unsupported-system", "Copied. Paste it into your document"], ["paste-off", "Copied. Pasting is off"]]) {
    show(copied("[@invented]", reason));
    expect(root.querySelector("strong")?.textContent).toBe(title);
  }
  show({ delivered: { outcome: "pasted", app: "Example Writer" }, text: "[@invented]" });
  expect(root.querySelector("strong")?.textContent).toBe("Pasted into Example Writer");
});

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

test("notice platform parsing retains the actual system", () => {
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
  show(copied("<img src=x onerror=alert(1)> [@a]"));
  expect(notice.querySelector("img")).toBeNull();
  expect(notice.querySelector("code")?.textContent).toBe("<img src=x onerror=alert(1)> [@a]");
  expect(notice.textContent).toContain("Copied");
  expect(notice.classList.contains("showing")).toBe(true);
  show(copied("[@b]"));
  expect(notice.querySelector("code")?.textContent).toBe("[@b]");
  expect(notice.querySelectorAll("code")).toHaveLength(1);
  const windows = document.createElement("div");
  renderNotice(windows, "windows")(copied("[@a]"));
  expect(windows.querySelector("strong")?.textContent).toBe("Copied. Press Ctrl+V to paste");
  expect(windows.textContent).not.toContain("⌘V");
});

test("the notice says where the citation was pasted, or why it was only copied", () => {
  expect(noticeTitle({ delivered: { outcome: "pasted", app: "Writer" }, text: "[@a]" })).toBe("Pasted into Writer");
  expect(noticeTitle({ delivered: { outcome: "pasted", app: null }, text: "[@a]" })).toBe("Pasted");
  expect(noticeTitle(copied("[@a]", "terminal"))).toBe("Copied. Not pasted into a terminal");
  expect(noticeTitle(copied("[@a]", "no-permission"))).toBe("Copied. Ibis needs your OK to paste");
  expect(noticeTitle(copied("[@a]", "key-release-failed"))).toBe("Copied. Press and release Ctrl and V");
  expect(noticeTitle(copied("[@a]", "paste-off"))).toBe("Copied. Pasting is off");
  expect(noticeTitle(copied("[@a]", "unsupported-system"))).toBe("Copied. Paste it into your document");
  expect(noticeTitle(copied("[@a]", "no-target"))).toBe("Copied. No app to paste into");
  expect(noticeTitle(copied("[@a]"), "macos")).toBe("Copied. Press ⌘V to paste");
  expect(noticeTitle(copied("[@a]", "unknown-future-code"))).toBe("Copied");
});

test.each(["macos", "windows", "linux"] as const)("%s receipt updates announce the result without moving document focus", (platform) => {
  const editor = document.createElement("input"), root = document.createElement("div");
  document.body.append(editor, root);
  try {
    editor.focus();
    const show = renderNotice(root, platform);
    show({ delivered: { outcome: "pasted", app: "Example Writer" }, text: "[@invented]" });
    expect(root.querySelector("strong")?.textContent).toBe("Pasted into Example Writer");
    show(copied("[@invented]", "paste-off"));
    expect(root.querySelector('[role="status"]')?.getAttribute("aria-live")).toBe("polite");
    expect(root.querySelector("strong")?.textContent).toBe("Copied. Pasting is off");
    expect(document.activeElement).toBe(editor);
  } finally { editor.remove(); root.remove(); }
});

test.each([
  ["unknown-app", "Copied. Automatic paste is not verified for this app"],
  ["target-changed", "Copied. The target app or window changed"],
  ["paste-expired", "Copied. The paste expired; paste it yourself"],
  ["secure-input", "Copied. Secure typing is on, possibly in another app. Press ⌘V to paste."],
  ["paste-unavailable", "Copied. Press ⌘V to paste."],
])("%s offers manual paste without claiming the target is a password field", (reason, message) => {
  const copy = copied("[@synthetic]", reason);
  expect(noticeTitle(copy)).toBe(message);
  const root = document.createElement("div");
  renderNotice(root, "macos")(copy);
  expect(root.querySelector("strong")?.textContent).toBe(message);
  expect(root.querySelector(".copied-notice")?.getAttribute("data-outcome")).toBe("copied");
});

test("the app's timer fades the notice out, and showing it again brings it back", () => {
  const root = document.createElement("div");
  const show = renderNotice(root, "linux");
  const notice = root.querySelector(".copied-notice")!;
  show(copied("[@a]"));
  leaveNotice(root);
  expect(notice.classList.contains("leaving")).toBe(true);
  show(copied("[@b]"));
  expect(notice.classList.contains("leaving")).toBe(false);
  expect(notice.classList.contains("showing")).toBe(true);
});

test("the notice window grants only copied text reading and event subscriptions", () => {
  expect(noticeCapability.windows).toEqual(["notice"]);
  expect([...noticeCapability.permissions].sort()).toEqual(["allow-copied-notice", "core:event:allow-listen", "core:event:allow-unlisten"]);
});

test("a citation that would be cut short moves to its own line, and only a paste is marked done", () => {
  const root = document.createElement("div");
  const show = renderNotice(root, "macos");
  const code = () => root.querySelector("code")!;
  const notice = () => root.querySelector<HTMLElement>(".copied-notice")!;
  // jsdom has no layout: stand in for a citation wider than its room.
  const widths = (scroll: number, client: number) => {
    Object.defineProperty(HTMLElement.prototype, "scrollWidth", { configurable: true, get() { return this.tagName === "CODE" ? scroll : 0; } });
    Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get() { return this.tagName === "CODE" ? client : 0; } });
  };
  try {
    widths(400, 60);
    show(copied("\\parencite{synthetic2020}", "no-permission"));
    expect(code().textContent).toBe("\\parencite{synthetic2020}");
    expect(notice().dataset.stacked).toBe("");
    expect(notice().dataset.outcome).toBe("copied");
    widths(120, 120);
    show({ delivered: { outcome: "pasted", app: "Writer" }, text: "[@synthetic]" });
    expect(notice().dataset.stacked).toBeUndefined();
    expect(notice().dataset.outcome).toBe("pasted");
  } finally {
    delete (HTMLElement.prototype as { scrollWidth?: number }).scrollWidth;
    delete (HTMLElement.prototype as { clientWidth?: number }).clientWidth;
  }
});
