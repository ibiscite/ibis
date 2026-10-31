import { act, fireEvent, render, screen } from "@testing-library/react";
import { createElement } from "react";
import { expect, onTestFinished, test, vi } from "vitest";
import type { LibraryStatus } from "./bindings";
import { libraryHealth, LibraryStatusLine, zoteroLibraryNames } from "./LibraryStatusLine";
import { ago, libraryMessages, recentUpdate } from "./messages";

const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
const now = new Date(2026, 9, 20, 12, 0).getTime();
const hour = 3_600_000;
const file = (overrides: Partial<LibraryStatus> = {}): LibraryStatus => ({
  revision: "1", state: "loaded", zotero: false, zotero_libraries: [], has_snapshot: true, name: "thesis.bib", snapshot_name: "thesis.bib",
  references: 1204, source_counts: { "synthetic-file": 1204 }, warnings: 0, error: null, problem: null, note: null, settings_notice: null, snapshot_zotero: false, stale: false,
  updated_at: null, checked_at: now - 3 * hour, ...overrides,
});
const zotero = (overrides: Partial<LibraryStatus> = {}) => file({
  zotero: true, snapshot_zotero: true, zotero_libraries: ["user"], name: "Zotero library", snapshot_name: "Zotero library", ...overrides,
});
const kept = "Zotero is not running, or it is not accepting connections; keeping the last loaded references";
const line = (status: LibraryStatus | null | undefined) =>
  [libraryMessages.summary(status), libraryMessages.age(status, now)].filter(Boolean).join(" · ");
const strip = (text: string | null) => text?.replace(/[⁨⁩]/g, "") ?? null;
const detail = (status: LibraryStatus) =>
  libraryMessages.detail(status, now, (s) => zoteroLibraryNames(s, (id) => `Group ${id}`)).map(strip);
const clockAt = (hours: number) => new Intl.DateTimeFormat("en", { timeStyle: "short" }).format(now - hours * hour);

test.each<[string, LibraryStatus | null | undefined, string, ReturnType<typeof libraryHealth>]>([
  ["starting", undefined, "Checking library…", "idle"],
  ["status unavailable", null, "Library details unavailable", "stop"],
  ["first run", file({ state: "none", has_snapshot: false, name: null, snapshot_name: null, references: 0, checked_at: null }), "No library chosen", "idle"],
  ["opening Zotero", zotero({ state: "loading", has_snapshot: false, snapshot_name: null, references: 0 }), "Zotero · opening…", "idle"],
  ["reloading keeps the count", file({ state: "reloading" }), "thesis.bib · 1,204 references · reloading…", "done"],
  ["just updated", zotero({ updated_at: now - 2 * 60_000, checked_at: now }), "Zotero · 1,204 references · updated 2 minutes ago", "done"],
  ["opened, never changed", zotero({ checked_at: now }), "Zotero · 1,204 references", "done"],
  ["a watched file", file(), "thesis.bib · 1,204 references", "done"],
  ["reading notices", file({ warnings: 2 }), "thesis.bib · 1,204 references · 2 reading notices", "look"],
  ["empty", zotero({ references: 0 }), "Zotero · no references", "look"],
  ["Zotero closed, copy kept", zotero({ state: "error", error: kept, stale: true, checked_at: now - 2 * hour }), "Zotero not updated · 2 h", "stop"],
  ["never opened", zotero({ state: "error", has_snapshot: false, snapshot_name: null, references: 0, error: "Zotero is not running" }), "Zotero · not opened", "stop"],
  ["failed switch between files", file({ state: "error", name: "missing.bib", error: "The library file is unavailable; keeping the last loaded references" }), "thesis.bib · 1,204 references · ⁨missing.bib⁩ not opened", "stop"],
  ["failed switch from a file to Zotero", file({ state: "error", zotero: true, zotero_libraries: ["user"], name: "Zotero library", error: "Zotero is not running" }), "thesis.bib · 1,204 references · ⁨Zotero⁩ not opened", "stop"],
  ["failed switch from Zotero to a file", zotero({ state: "error", zotero: false, zotero_libraries: [], name: "missing.bib", error: "The library file is unavailable" }), "Zotero · 1,204 references · ⁨missing.bib⁩ not opened", "stop"],
])("%s", (_, status, expected, health) => {
  expect(line(status)).toBe(expected);
  expect(libraryHealth(status)).toBe(health);
});

test("the age leaves once the update is no longer news", () => {
  expect(libraryMessages.age(file({ updated_at: now - recentUpdate + 1 }), now)).toBe("updated 10 minutes ago");
  expect(libraryMessages.age(file({ updated_at: now - recentUpdate - 1 }), now)).toBeNull();
});

test("relative times are short, never in the future, and turn into dates after a week", () => {
  expect(ago(now + hour, now)).toBe("just now");
  expect(ago(now - 44_000, now)).toBe("just now");
  expect(ago(now - 60_000, now)).toBe("1 minute ago");
  expect(ago(now - 2 * hour, now)).toBe("2 hours ago");
  expect(ago(now - 48 * hour, now)).toBe("2 days ago");
  expect(ago(now - 24 * 8 * hour, now)).toBe("on Oct 12");
});

test("older check dates distinguish the same month and day in different years", () => {
  expect(ago(new Date(2025, 9, 12, 12).getTime(), now)).toBe("on Oct 12, 2025");
  expect(ago(new Date(2026, 9, 12, 12).getTime(), now)).toBe("on Oct 12");
});

test("date years follow the local calendar across New Year without changing future times", () => {
  const january = new Date(2026, 0, 9, 1, 30).getTime();
  expect(ago(new Date(2026, 0, 1, 1, 30).getTime(), january)).toBe("on Jan 1");
  expect(ago(new Date(2025, 11, 31, 23, 30).getTime(), january)).toBe("on Dec 31, 2025");
  expect(ago(new Date(2027, 0, 1, 1, 30).getTime(), january)).toBe("just now");
});

test("the detail is a short line, then libraries, check time and any problem", () => {
  expect(detail(zotero({ zotero_libraries: ["user", { group: 41 }, { group: 9 }], checked_at: now - 3 * hour }))).toEqual(
    ["Zotero · 1,204 references", `My Library, Group 41, Group 9 · checked 3 hours ago, ${clockAt(3)}`]);
  expect(detail(file({ warnings: 1 }))).toEqual(["thesis.bib · 1,204 references · 1 reading notice", `checked 3 hours ago, ${clockAt(3)}`]);
  expect(detail(zotero({ state: "error", error: kept, stale: true, checked_at: now - 2 * hour }))).toEqual(
    ["Zotero · 1,204 references", `My Library · last read 2 hours ago, ${clockAt(2)} · Zotero library could not be read. ${kept}. Open Zotero, then reload.`]);
  expect(detail(file({ state: "error", name: "missing.bib", error: "The library file is unavailable" }))).toEqual(
    ["thesis.bib · 1,204 references · missing.bib not opened", `checked 3 hours ago, ${clockAt(3)} · The library could not be opened: missing.bib. The library file is unavailable. Choose it again or reconnect the drive.`]);
  expect(detail(file({ state: "error", zotero: true, zotero_libraries: ["user"], name: "Zotero library", error: "Zotero is not running" }))).toEqual(
    ["thesis.bib · 1,204 references · Zotero not opened", `checked 3 hours ago, ${clockAt(3)} · Zotero library could not be read. Zotero is not running. Open Zotero, then reload.`]);
  expect(detail(file({ state: "none", has_snapshot: false, name: null }))).toEqual(["No library chosen", null]);
});

test("names are isolated so right-to-left names keep their place", () => {
  const [first, second] = libraryMessages.detail(zotero({ zotero_libraries: ["user", { group: 2 }] }), now,
    (s) => zoteroLibraryNames(s, () => "مجموعة 2"));
  expect(first).toBe("⁨Zotero⁩ · 1,204 references");
  expect(second).toMatch(/^⁨My Library⁩, ⁨مجموعة 2⁩ · /);
  render(createElement(LibraryStatusLine, { library: file({ name: "مراجع.bib", snapshot_name: "مراجع.bib" }), groupName: String }));
  const status = screen.getByRole("status", { name: "Library status" });
  expect(status.querySelector("bdi")?.textContent).toBe("مراجع.bib");
  expect(status.textContent).toBe("مراجع.bib · 1,204 references");
});

test.each(["مراجع.bib", "מקורות.bib", "研究.bib", "example.bib"])("the English status phrase keeps source, state and age order around %s", (name) => {
  const { container } = render(createElement("div", { dir: "rtl" }, createElement(LibraryStatusLine, {
    library: file({ name, snapshot_name: name, updated_at: Date.now() }), groupName: String,
  })));
  const words = container.querySelector(".library-words")!;
  expect(words.tagName).toBe("BDI");
  expect(words.getAttribute("dir")).toBe("ltr");
  const phrase = screen.getByRole("status", { name: "Library status" });
  expect(phrase.getAttribute("dir")).toBe("ltr");
  expect(phrase.textContent).toBe(`${name} · 1,204 references`);
  expect(phrase.querySelector(".library-source")?.tagName).toBe("BDI");
  const age = words.querySelector(".library-age")!;
  expect(age.tagName).toBe("BDI");
  expect(age.getAttribute("dir")).toBe("auto");
  expect(age.textContent).toBe(" · updated just now");
  expect(age.getAttribute("aria-hidden")).toBe("true");
  // The clock remains outside the live announcement and has a non-live full detail.
  expect(phrase.contains(age)).toBe(false);
  expect(container.querySelector(".library-detail")?.textContent).toContain("updated just now");
});

test("screen readers can read the detail, including the count kept after a failed update", () => {
  // The fixture times are fixed; a real clock makes "3 hours ago" a date once they age.
  vi.useFakeTimers({ now, toFake: ["Date"] });
  onTestFinished(() => { vi.useRealTimers(); });
  const { container } = render(createElement(LibraryStatusLine, {
    // The line reads the real clock, so the check time is relative to it, not to the fixed `now`.
    library: file({ state: "error", stale: true, error: "The library file is unavailable; keeping the last loaded references", references: 4, checked_at: Date.now() - 3 * hour }),
    groupName: String,
  }));
  expect(container.querySelector(".library-status")?.textContent).toMatch(/^thesis\.bib not updated · /);
  expect(strip(container.querySelector(".library-detail")?.textContent ?? null)).toMatch(
    /^thesis\.bib · 4 references\. last read .+ ago, .+ · The library could not be opened: thesis\.bib\. The library file is unavailable; keeping the last loaded references\. Choose it again or reconnect the drive\.$/);
});

test("every footer text sits in an element WebKitGTK exposes, not a plain span beside one", () => {
  // Linux screen readers lost the count and the detail when they were plain spans (#531).
  const { container } = render(createElement(LibraryStatusLine, { library: file({ updated_at: now }), groupName: String }));
  const words = container.querySelector(".library-name");
  expect([...(words?.children ?? [])].map((child) => child.tagName)).toEqual(["BDI", "BDI", "BDI"]);
  // The separator is its own item between name and state, so a right-to-left
  // page keeps it between them instead of isolating it with the state's words.
  expect([...(words?.children ?? [])].map((child) => child.textContent)).toEqual(["thesis.bib", " · ", "1,204 references"]);
  expect(container.querySelector(".library-detail")?.tagName).toBe("P");
});

test("missing fields from an older backend show no age instead of a wrong one", () => {
  const old = { ...file(), updated_at: undefined, checked_at: undefined, stale: undefined, snapshot_zotero: undefined } as unknown as LibraryStatus;
  expect(line(old)).toBe("thesis.bib · 1,204 references");
  expect(detail(old)).toEqual(["thesis.bib · 1,204 references", null]);
});

test("the footer age ticks by itself, skips hidden windows and catches up when shown", () => {
  vi.useFakeTimers({ now });
  let hidden = false;
  const visibility = vi.spyOn(document, "visibilityState", "get").mockImplementation(() => (hidden ? "hidden" : "visible"));
  try {
    render(createElement(LibraryStatusLine, { library: zotero({ updated_at: now, checked_at: now }), groupName: (id: number) => `Group ${id}` }));
    const item = screen.getByRole("status", { name: "Library status" }).closest(".library-status")!;
    expect(item.textContent).toBe("Zotero · 1,204 references · updated just now");
    act(() => { vi.advanceTimersByTime(3 * 60_000); });
    expect(item.textContent).toBe("Zotero · 1,204 references · updated 3 minutes ago");
    hidden = true;
    act(() => { vi.advanceTimersByTime(3 * 60_000); });
    expect(item.textContent).toBe("Zotero · 1,204 references · updated 3 minutes ago");
    hidden = false;
    act(() => { document.dispatchEvent(new Event("visibilitychange")); });
    expect(item.textContent).toBe("Zotero · 1,204 references · updated 6 minutes ago");
    act(() => { vi.setSystemTime(now + recentUpdate + 1); window.dispatchEvent(new Event("focus")); });
    expect(item.textContent).toBe("Zotero · 1,204 references");
  } finally {
    visibility.mockRestore();
    vi.useRealTimers();
  }
});

test("native hide removes the age timer; reopen catches up without repeating the live status", async () => {
  vi.useFakeTimers({ now });
  try {
    const { container, unmount, rerender } = render(createElement(LibraryStatusLine, {
      library: zotero({ state: "error", stale: true, error: kept, checked_at: now - 2 * 60_000 }), groupName: String,
    }));
    const live = screen.getByRole("status", { name: "Library status" });
    const changes: (string | null)[] = [];
    const observer = new MutationObserver(() => changes.push(live.textContent));
    observer.observe(live, { childList: true, subtree: true, characterData: true });
    expect(vi.getTimerCount()).toBe(1);
    await act(async () => { vi.advanceTimersByTime(30_000); });
    expect(container.querySelector(".library-age")?.textContent).toContain("3 min");
    expect(changes).toHaveLength(0);
    await act(async () => { document.documentElement.dataset.pickerHidden = ""; });
    expect(vi.getTimerCount()).toBe(0);
    await act(async () => { vi.advanceTimersByTime(180_000); });
    expect(container.querySelector(".library-age")?.textContent).toContain("3 min");
    await act(async () => { delete document.documentElement.dataset.pickerHidden; });
    expect(container.querySelector(".library-age")?.textContent).toContain("6 min");
    expect(vi.getTimerCount()).toBe(1);
    expect(changes).toHaveLength(0);
    await act(async () => { rerender(createElement(LibraryStatusLine, { library: zotero(), groupName: String })); });
    expect(changes).toEqual(["Zotero · 1,204 references"]);
    expect(live.textContent).toBe("Zotero · 1,204 references");
    await act(async () => { vi.advanceTimersByTime(30_000); });
    expect(changes).toEqual(["Zotero · 1,204 references"]);
    observer.disconnect();
    unmount();
    expect(vi.getTimerCount()).toBe(0);
  } finally { delete document.documentElement.dataset.pickerHidden; vi.useRealTimers(); }
});

test("the non-empty Library button keeps one separate announcement, and its age never repeats it", async () => {
  vi.useFakeTimers({ now, toFake: ["Date", "setInterval", "clearInterval"] });
  const menu = { label: "Library", isOpen: false, onOpenChange: vi.fn(), children: null };
  let current = file({ state: "error", stale: true, problem: "missing", error: "The library file is unavailable", checked_at: now - 2 * 60_000 });
  const { container, rerender, unmount } = render(createElement(LibraryStatusLine, { library: current, groupName: String, menu }));
  const live = screen.getByRole("status", { name: "Library status" });
  const button = screen.getByRole("button", { name: "Library" });
  const announcements: (string | null)[] = [];
  const names: string[] = [];
  const spoken = new MutationObserver(() => announcements.push(live.textContent));
  const named = new MutationObserver(() => names.push(button.getAttribute("aria-label")!));
  spoken.observe(live, { childList: true, subtree: true, characterData: true });
  named.observe(button, { attributes: true, attributeFilter: ["aria-label"] });
  try {
    expect(button.textContent).toBe("thesis.bib moved · 2 min");
    expect(button.contains(live)).toBe(false);
    expect(live.closest('[aria-hidden="true"]')).toBeNull();
    await act(async () => { vi.advanceTimersByTime(30_000); });
    expect(container.querySelector(".library-age")?.textContent).toContain("3 min");
    expect(announcements).toEqual([]);
    current = { ...current, state: "loaded", stale: false, error: null, problem: null, references: 13 };
    await act(async () => { rerender(createElement(LibraryStatusLine, { library: current, groupName: String, menu })); });
    expect(announcements).toEqual(["thesis.bib · 13 references"]);
    expect(names).toEqual([]);
    expect(screen.getAllByRole("status", { name: "Library status" })).toHaveLength(1);
  } finally {
    spoken.disconnect(); named.disconnect(); unmount(); vi.useRealTimers();
  }
});

test("no sizing rule reaches the hidden spoken detail, so the footer never widens the page", () => {
  // `.library-bar p { width: 100% }` once matched it and beat .visually-hidden (#531).
  const styles = readFileSync(new FileURL("./styles.css", import.meta.url), "utf8")
    .replace(/\/\*[\s\S]*?\*\//g, "");
  const bar = document.createElement("section");
  bar.className = "library-bar";
  document.body.append(bar);
  const { container } = render(createElement(LibraryStatusLine, { library: file({ updated_at: now }), groupName: String }), { container: bar });
  const hidden = container.querySelector(".library-detail");
  expect(hidden).not.toBeNull();
  const sizing = [...styles.matchAll(/([^{}]+)\{([^}]*)\}/g)]
    .filter(([, , body]) => /(^|[;\s])(width|min-width|margin|padding|position)\s*:/.test(body ?? ""))
    .flatMap(([, selector]) => (selector ?? "").split(",").map((part) => part.trim()))
    .filter((selector) => selector.includes("library") && !selector.startsWith("@"));
  const reaching = sizing.filter((selector) => { try { return hidden?.matches(selector); } catch { return false; } });
  expect(reaching).toEqual([]);
  bar.remove();
});

test("the footer shortens the age, then the name, and only then the state words, with an ellipsis", () => {
  const styles = readFileSync(new FileURL("./styles.css", import.meta.url), "utf8");
  const rule = (selector: string) => styles.match(new RegExp(`^\\${selector} \\{([^}]*)\\}`, "m"))?.[1] ?? "";
  const shrink = (selector: string) => Number(rule(selector).match(/flex-shrink: ([\d.]+)|flex: 0 ([\d.]+) auto/)?.slice(1).find(Boolean));
  // The age has no width of its own: it only fills what the name and state leave.
  expect(rule(".library-age")).toContain("flex: 1 1 0");
  expect(shrink(".library-source")).toBeGreaterThan(1);
  expect(rule(".library-state")).toContain("flex: 0 1 auto");
  for (const selector of [".library-source", ".library-state", ".library-age"]) {
    expect(rule(selector)).toContain("text-overflow: ellipsis");
    expect(rule(selector)).toContain("overflow: hidden");
  }
});

test("the visible age is hidden from screen readers, which read the detail instead", () => {
  const { container } = render(createElement(LibraryStatusLine, { library: zotero({ updated_at: Date.now(), checked_at: Date.now() }), groupName: String }));
  expect(container.querySelector(".library-age")?.getAttribute("aria-hidden")).toBe("true");
  expect(strip(container.querySelector(".library-detail")?.textContent ?? null)).toMatch(
    /^Zotero · 1,204 references · updated just now\. My Library · checked just now, /);
  // A kept copy says its age once, in the detail.
  const kept = render(createElement(LibraryStatusLine, {
    library: zotero({ state: "error", stale: true, error: "Zotero is not running; keeping the last loaded references", checked_at: Date.now() - 2 * hour }),
    groupName: String,
  }));
  const heard = strip(kept.container.querySelector(".library-detail")?.textContent ?? null) ?? "";
  expect(heard.match(/last read 2 hours ago/g)).toHaveLength(1);
});

test("the status dot sits at the footer edge, 12 px before the text and centred on its x-height", () => {
  const styles = readFileSync(new FileURL("./styles.css", import.meta.url), "utf8");
  expect(styles).toMatch(/^\.library-status \{[^}]*align-items: baseline; gap: var\(--space-3\);/m);
  const dot = styles.match(/^\.status-dot \{([^}]*)\}/m)?.[1];
  expect(dot).not.toMatch(/margin/);
  expect(dot).toMatch(/translate: 0 calc\(var\(--status-dot-size\) \/ 2 - \.5ex\);/);
});

test.each(["ltr", "rtl"])("the native Library trigger reserves its own hit area beside Find in a narrow %s footer", (dir) => {
  // jsdom cannot lay out pixels, but it can check the native button's layout
  // contract: an in-flow box and a non-shrinking minimum around visible content.
  const style = document.createElement("style");
  style.textContent = readFileSync(new FileURL("./ui/Button.css", import.meta.url), "utf8") +
    readFileSync(new FileURL("./ui/LibraryStatus.css", import.meta.url), "utf8");
  document.head.append(style);
  onTestFinished(() => style.remove());
  const open = vi.fn(), find = vi.fn();
  render(createElement("div", { dir, style: { display: "flex", width: 80 } }, createElement(LibraryStatusLine, {
    library: file({ state: "error", stale: true, problem: "missing", error: "the library file is unavailable; keeping the last loaded references" }),
    groupName: String,
    action: createElement("button", { className: "source-action library-inline-action", onClick: find }, "Find thesis.bib…"),
    menu: { label: "Library", isOpen: false, onOpenChange: open, children: null },
  })));
  const trigger = screen.getByRole("button", { name: "Library" });
  expect(trigger.textContent).toContain("thesis.bib moved");
  expect(getComputedStyle(trigger).position).toBe("relative");
  expect(getComputedStyle(trigger.parentElement!).minWidth).toBe("var(--control-min-size)");
  fireEvent.click(trigger, { detail: 0 });
  expect(open).toHaveBeenCalledWith(true);
  expect(find).not.toHaveBeenCalled();
});
