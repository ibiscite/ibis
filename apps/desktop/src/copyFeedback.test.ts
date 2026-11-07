import { expect, test, vi } from "vitest";

test("paste milestones decorate only the receipt heading and preserve the citation and focus", () => {
  const root = document.createElement("div");
  const focus = document.activeElement;
  const show = renderNotice(root, "macos");
  for (const [milestone, label] of [[1, "First citation"], [100, "Citation 100"], [1000, "Citation 1,000"]] as const) {
    show({ delivered: { outcome: "pasted", app: "Invented Writer" }, text: "[@invented]", milestone });
    expect(root.querySelector("strong")?.textContent).toBe(`Pasted into Invented Writer · ${label}`);
    expect(root.querySelector("code")?.textContent).toBe("[@invented]");
    expect(document.activeElement).toBe(focus);
  }
  show({ delivered: { outcome: "pasted", app: "Invented Writer" }, text: "[@next]" });
  expect(root.querySelector("strong")?.textContent).toBe("Pasted into Invented Writer");
});
import type { NoticeMessage } from "./api";
import { say } from "./i18n";
import { afterCopy, COPY_REASONS, type CopyReason, DRAG_DISTANCE, leaveNotice, type Notice, noticePlatform, noticeTitle, readNotice, renderNotice, rowClickAction, type RowClick } from "./copyFeedback";

const copied = (text: string, reason: CopyReason = "paste-unavailable"): Notice => ({ delivered: { outcome: "copied", reason }, text });
import noticeCapability from "../src-tauri/capabilities/notice.json";

test.each([["macos", "⌘V"], ["windows", "Ctrl+V"], ["linux", "Ctrl+V"]] as const)("plain-copy receipts always use %s's paste key, including after reopening", (platform, key) => {
  const root = document.createElement("div"), show = renderNotice(root, platform);
  for (let copy = 0; copy < 4; copy++) {
    show(copied(`[@invented${copy}]`));
    expect(root.querySelector("strong")?.textContent).toBe(`Copied. Ibis couldn’t confirm where the text would go. Press ${key} to paste.`);
  }
  renderNotice(root, platform)(copied("[@reopened]"));
  expect(root.querySelector("strong")?.textContent).toBe(`Copied. Ibis couldn’t confirm where the text would go. Press ${key} to paste.`);
});

test("on Linux every refusal keeps its reason and ends with Ctrl+V, and a paste names the app", () => {
  const root = document.createElement("div"), show = renderNotice(root, "linux");
  for (const [reason, title] of [["terminal", "Copied. Not pasted into a terminal. Press Ctrl+V to paste."],
    ["unsupported-system", "Copied. Ibis can’t paste on this system yet. Press Ctrl+V to paste."], ["paste-off", "Copied. Pasting is off in Settings. Press Ctrl+V to paste."]] as const) {
    show(copied("[@invented]", reason));
    expect(root.querySelector("strong")?.textContent).toBe(title);
  }
  show({ delivered: { outcome: "pasted", app: "Example Writer" }, text: "[@invented]" });
  expect(root.querySelector("strong")?.textContent).toBe("Pasted into Example Writer");
});

test("only unpinned copies without blocking warnings hide the picker", () => {
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
  expect(windows.querySelector("strong")?.textContent).toBe("Copied. Ibis couldn’t confirm where the text would go. Press Ctrl+V to paste.");
  expect(windows.textContent).not.toContain("⌘V");
});

test("the notice says where the citation was pasted, or why it was only copied", () => {
  expect(noticeTitle({ delivered: { outcome: "pasted", app: "Writer" } })).toBe("Pasted into Writer");
  expect(noticeTitle({ delivered: { outcome: "pasted", app: null } })).toBe("Pasted");
  expect(noticeTitle(copied("[@a]", "terminal"), "macos")).toBe("Copied. Not pasted into a terminal. Press ⌘V to paste.");
});

test("a reason Rust sends that this build does not know is reported and still says why and how to paste", () => {
  const error = vi.spyOn(console, "error").mockImplementation(() => {});
  try {
    const wire: NoticeMessage = { delivered: { outcome: "copied", reason: "unknown-future-code" }, text: "[@a]" };
    const notice = readNotice(wire);
    expect(notice.delivered).toEqual({ outcome: "copied", reason: "paste-unavailable" });
    expect(noticeTitle(notice, "macos")).toBe("Copied. Ibis couldn’t confirm where the text would go. Press ⌘V to paste.");
    expect(error).toHaveBeenCalledExactlyOnceWith("Unknown copy reason: unknown-future-code");
    expect(readNotice({ delivered: { outcome: "copied", reason: "terminal" }, text: "[@a]" }).delivered).toEqual({ outcome: "copied", reason: "terminal" });
    expect(error).toHaveBeenCalledOnce();
  } finally { error.mockRestore(); }
});

test("the copy reasons are exactly the ones Rust sends, and each has its words", async () => {
  const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
  const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
  const read = (path: string) => readFileSync(new FileURL(path, import.meta.url), "utf8");
  const core = read("../../../crates/ibis-core/src/destination/mod.rs").match(/pub enum CopyReason \{([^}]*)\}/)![1]!
    .replace(/\/\/[^\n]*/g, "").match(/[A-Z]\w*/g)!.map((name) => name.replace(/(?<!^)[A-Z]/g, (letter) => `-${letter}`).toLowerCase());
  const delivery = [...read("../src-tauri/src/paste/delivery.rs").matchAll(/reason: "([a-z-]+)"\.into\(\)/g)].map((match) => match[1]!);
  expect(core).toContain("paste-off");
  expect([...COPY_REASONS].sort()).toEqual([...new Set([...core, ...delivery])].sort());
});

test("every reason reads as the Notices table in docs/design/delivery.md says: why, then its fix", async () => {
  const reasons: Record<string, string> = Object.fromEntries(COPY_REASONS.map((reason) => [reason, say("copied-notice-reason", { reason })]));
  const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
  const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
  const design = readFileSync(new FileURL("../../../docs/design/delivery.md", import.meta.url), "utf8");
  const section = design.slice(design.indexOf("## Notices"));
  const rows = section.split("\n").filter((line) => /^\| [^-]/.test(line)).slice(1).map((line) => line.split("|").slice(1, -1).map((cell) => cell.trim()));
  const table = Object.fromEntries(rows.map(([, code, words]) => [code!.replaceAll("`", ""), words]));
  expect(table).toEqual(reasons);
  for (const words of Object.values(reasons)) {
    // Copied, a sentence saying why, then the fix: never the fix alone.
    expect(words).toMatch(/^Copied\. [^.]+\. .*\.$/);
    expect(words).not.toMatch(/^Copied\. (Press|Use|Select)/);
    expect(words).not.toMatch(/password/i);
    expect(words).not.toContain("'");
  }
  // Where the fix is pasting, the same words say so, in the system's key.
  const pastes = Object.entries(reasons).filter(([, words]) => words.includes("⌘V to paste")).map(([reason]) => reason as CopyReason);
  expect(pastes.length).toBeGreaterThan(15);
  for (const reason of pastes) expect(noticeTitle(copied("[@a]", reason), "macos")).toMatch(/ Press ⌘V to paste\.$/);
  // Each system shows the table's words whole, in its own paste key.
  for (const [reason, words] of Object.entries(reasons) as [CopyReason, string][]) {
    expect(noticeTitle(copied("[@a]", reason), "macos")).toBe(words);
    for (const platform of ["windows", "linux"] as const) expect(noticeTitle(copied("[@a]", reason), platform)).toBe(words.replaceAll("⌘V", "Ctrl+V"));
  }
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
    expect(root.querySelector("strong")?.textContent).toBe(`Copied. Pasting is off in Settings. Press ${platform === "macos" ? "⌘V" : "Ctrl+V"} to paste.`);
    expect(document.activeElement).toBe(editor);
  } finally { editor.remove(); root.remove(); }
});

test.each<[CopyReason, string]>([
  ["unknown-app", "Copied. Automatic paste is not approved for this app. Press ⌘V to paste."],
  ["target-changed", "Copied. The target app or window changed. Press ⌘V to paste."],
  ["clipboard-changed", "Copied. The clipboard changed since. Copy the citation again."],
  ["paste-expired", "Copied. The paste expired. Press ⌘V to paste."],
  ["secure-input", "Copied. Secure typing is on, possibly in another app. Press ⌘V to paste."],
  ["paste-unavailable", "Copied. Ibis couldn’t confirm where the text would go. Press ⌘V to paste."],
  ["selection-not-shown", "Copied. Ibis replaces only text it showed you. Press ⌘V to replace it."],
  ["citation-not-selected", "Copied. Ibis replaces a citation only when it is selected. Select it, then press ⌘V."],
])("%s offers manual paste without claiming the target is a password field", (reason, message) => {
  const copy = copied("[@synthetic]", reason);
  expect(noticeTitle(copy, "macos")).toBe(message);
  const root = document.createElement("div");
  renderNotice(root, "macos")(copy);
  expect(root.querySelector("strong")?.textContent).toBe(message);
  expect(root.querySelector(".copied-notice")?.getAttribute("data-outcome")).toBe("copied");
});

test("the first paste into a word processor adds the plugin note, once, in the status region", () => {
  const root = document.createElement("div");
  const show = renderNotice(root, "macos");
  show({ delivered: { outcome: "pasted", app: "Example Writer" }, text: "(Invented, 2026)", plugin_note: true });
  const note = root.querySelector('[role="status"] .copy-receipt-note');
  expect(note?.textContent).toContain("insert with the plugin");
  expect(note?.textContent).toContain("Zotero, Mendeley, EndNote, Citavi, Paperpile");
  show({ delivered: { outcome: "pasted", app: "Example Writer" }, text: "(Invented, 2026)" });
  expect(root.querySelector(".copy-receipt-note")).toBeNull();
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

test("every receipt uses one card and only a paste is marked done", () => {
  const root = document.createElement("div");
  const show = renderNotice(root, "macos");
  const code = () => root.querySelector("code")!;
  const notice = () => root.querySelector<HTMLElement>(".copied-notice")!;
  show(copied("\\parencite{synthetic2020}", "no-permission"));
  expect(code().textContent).toBe("\\parencite{synthetic2020}");
  expect(notice().classList.contains("copy-receipt")).toBe(true);
  expect(notice().querySelectorAll(".copy-receipt-mark")).toHaveLength(1);
  expect(notice().dataset.outcome).toBe("copied");
  show({ delivered: { outcome: "pasted", app: "Writer" }, text: "[@synthetic]" });
  expect(notice().dataset.outcome).toBe("pasted");
  expect(notice().querySelectorAll("code")).toHaveLength(1);
});
