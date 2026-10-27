import { afterEach, expect, test, vi } from "vitest";

const native = vi.hoisted(() => ({ listen: vi.fn(async (_name: string, _handler: unknown) => () => {}), copiedNotice: vi.fn(async () => ({ delivered: { outcome: "copied", reason: null }, text: "[@synthetic]" })) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: native.listen }));
vi.mock("./bindings", () => ({ commands: { copiedNotice: native.copiedNotice } }));

afterEach(() => {
  vi.restoreAllMocks();
  vi.clearAllMocks();
  document.body.replaceChildren();
  document.documentElement.classList.remove("notice-window");
  for (const name of ["platform", "system", "surface"]) delete document.documentElement.dataset[name];
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
  expect(document.querySelector(".copied-notice strong")?.textContent).toBe("Copied");
  expect(native.listen.mock.calls.map(([name]) => name)).toEqual(["notice:leave", "notice:show"]);
  expect(native.copiedNotice).toHaveBeenCalledOnce();
  expect(native.copiedNotice.mock.invocationCallOrder[0]).toBeGreaterThan(native.listen.mock.invocationCallOrder[1]!);
});
