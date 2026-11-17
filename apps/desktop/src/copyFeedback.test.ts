import { expect, test } from "vitest";
import { say } from "./i18n";
import { COPY_REASONS, type CopyReason, type Notice, noticeTitle, renderNotice } from "./copyFeedback";

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

test("the notice says where the citation was pasted, or why it was only copied", () => {
  expect(noticeTitle({ delivered: { outcome: "pasted", app: "Writer" } })).toBe("Pasted into Writer");
  expect(noticeTitle({ delivered: { outcome: "pasted", app: null } })).toBe("Pasted");
  expect(noticeTitle(copied("[@a]", "terminal"), "macos")).toBe("Copied. Not pasted into a terminal. Press ⌘V to paste.");
});

test("every reason says why, then its fix, in the system's paste key", () => {
  const reasons: Record<string, string> = Object.fromEntries(COPY_REASONS.map((reason) => [reason, say("copied-notice-reason", { reason })]));
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

test("the notice window grants only copied text reading and event subscriptions", () => {
  expect(noticeCapability.windows).toEqual(["notice"]);
  expect([...noticeCapability.permissions].sort()).toEqual(["allow-change-suggested-format", "allow-copied-notice", "core:event:allow-listen", "core:event:allow-unlisten"]);
});
