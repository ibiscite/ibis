import { afterEach, expect, test, vi } from "vitest";

const native = vi.hoisted(() => ({ listen: vi.fn(async (_name: string, _handler: unknown) => () => {}), changeSuggestedFormat: vi.fn(async () => null), copiedNotice: vi.fn(async () => ({ delivered: { outcome: "copied", reason: "paste-unavailable" }, text: "[@synthetic]" })) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: native.listen }));
vi.mock("./bindings", async (actual) => ({ ...await actual<typeof import("./bindings")>(), commands: { copiedNotice: native.copiedNotice, changeSuggestedFormat: native.changeSuggestedFormat } }));

afterEach(() => {
  vi.restoreAllMocks();
  vi.clearAllMocks();
  document.body.replaceChildren();
  document.documentElement.classList.remove("notice-window");
  for (const name of ["platform", "system", "surface", "appearance"]) delete document.documentElement.dataset[name];
  window.history.replaceState(null, "", "/");
});

test.each([["MacIntel", "macos"], ["Win32", "windows"], ["Linux x86_64", "linux"]])("%s notice shares the macOS look while retaining its actual system", async (name, system) => {
  vi.spyOn(navigator, "platform", "get").mockReturnValue(name);
  document.documentElement.classList.add("notice-window");
  document.body.innerHTML = '<div id="notice"></div>';
  window.history.replaceState(null, "", "/?surface=opaque");
  vi.resetModules();
  await import("./notice");
  await vi.waitFor(() => expect(document.querySelector(".copied-notice code")?.textContent).toBe("[@synthetic]"));
  expect(document.documentElement.dataset.platform).toBe("macos");
  expect(document.documentElement.dataset.system).toBe(system);
  expect(document.documentElement.dataset.surface).toBe("opaque");
  expect(document.querySelector(".copied-notice strong")?.textContent).toBe(`Copied. Ibis couldn’t confirm where the text would go. Press ${system === "macos" ? "⌘V" : "Ctrl+V"} to paste.`);
  expect(native.listen.mock.calls.map(([name]) => name)).toEqual(["notice:leave", "notice:show"]);
  expect(native.copiedNotice).toHaveBeenCalledOnce();
  expect(native.copiedNotice.mock.invocationCallOrder[0]).toBeGreaterThan(native.listen.mock.invocationCallOrder[1]!);
});

test("the notice follows Ibis's own Light or Dark, not only the system's", async () => {
  vi.spyOn(navigator, "platform", "get").mockReturnValue("MacIntel");
  document.documentElement.classList.add("notice-window");
  document.body.innerHTML = '<div id="notice"></div>';
  native.copiedNotice.mockResolvedValueOnce({ delivered: { outcome: "copied", reason: "paste-unavailable" }, text: "[@synthetic]", appearance: "dark" } as never);
  vi.resetModules();
  await import("./notice");
  await vi.waitFor(() => expect(document.documentElement.dataset.appearance).toBe("dark"));
  // A later notice with Ibis set to System follows the Mac again.
  const show = native.listen.mock.calls.find(([name]) => name === "notice:show")![1] as (event: { payload: unknown }) => void;
  show({ payload: { delivered: { outcome: "copied", reason: "paste-unavailable" }, text: "[@synthetic]" } });
  expect(document.documentElement.dataset.appearance).toBe("system");
});

test("a first suggested paste names the format and offers Change only for that notice", async () => {
  document.body.innerHTML = '<div id="notice"></div>';
  native.copiedNotice.mockResolvedValueOnce({ delivered: { outcome: "pasted", app: "Word" }, text: "(García, 2021)", suggested_format: "apa" } as never);
  vi.resetModules();
  await import("./notice");
  await vi.waitFor(() => expect(document.querySelector(".copied-notice strong")?.textContent).toContain("Pasted in APA"));
  const change = document.querySelector<HTMLButtonElement>(".copied-notice button")!;
  expect(change.textContent).toBe("Change");
  change.click();
  expect(native.changeSuggestedFormat).toHaveBeenCalledOnce();
  const show = native.listen.mock.calls.find(([name]) => name === "notice:show")![1] as (event: { payload: unknown }) => void;
  show({ payload: { delivered: { outcome: "pasted", app: "Word" }, text: "(García, 2021)" } });
  expect(document.querySelector(".copied-notice button")).toBeNull();
});

test("a Linux suggested copy keeps its reason and offers the suggestion in the same notice without taking focus", async () => {
  vi.spyOn(navigator, "platform", "get").mockReturnValue("Linux x86_64");
  document.body.innerHTML = '<input aria-label="Editor"><div id="notice"></div>';
  const editor = document.querySelector("input")!;
  editor.focus();
  native.copiedNotice.mockResolvedValueOnce({ delivered: { outcome: "copied", reason: "unsupported-system" }, text: "(Example, 2026)", suggested_format: "apa" } as never);
  vi.resetModules();
  await import("./notice");
  await vi.waitFor(() => expect(document.querySelector(".copied-notice code")?.textContent).toBe("(Example, 2026)"));
  expect(document.querySelectorAll('[role="status"]')).toHaveLength(1);
  expect(document.querySelector(".copied-notice strong")?.textContent).toBe("Copied. Ibis can’t paste on this system yet. Press Ctrl+V to paste.");
  expect(document.querySelector(".copy-receipt-note")?.textContent).toBe("Suggested APA · Change");
  expect(document.activeElement).toBe(editor);
  document.querySelector<HTMLButtonElement>(".copied-notice button")!.click();
  expect(native.changeSuggestedFormat).toHaveBeenCalledOnce();
  const show = native.listen.mock.calls.find(([name]) => name === "notice:show")![1] as (event: { payload: unknown }) => void;
  show({ payload: { delivered: { outcome: "copied", reason: "unsupported-system" }, text: "(Example, 2026)" } });
  expect(document.querySelector(".copy-receipt-note")).toBeNull();
  expect(document.querySelector(".copied-notice button")).toBeNull();
});
