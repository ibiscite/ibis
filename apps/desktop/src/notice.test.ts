import { afterEach, expect, test, vi } from "vitest";

const native = vi.hoisted(() => ({ listen: vi.fn(async (_name: string, _handler: unknown) => () => {}), changeSuggestedFormat: vi.fn(async () => null), copiedNotice: vi.fn(async () => ({ delivered: { outcome: "copied", reason: "paste-unavailable" }, text: "[@synthetic]", language: "en-US" })) }));
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

test("a first suggested paste names the format and offers Change only for that notice", async () => {
  document.body.innerHTML = '<div id="notice"></div>';
  native.copiedNotice.mockResolvedValueOnce({ delivered: { outcome: "pasted", app: "Word" }, text: "(García, 2021)", suggested_format: "apa", language: "en-US" } as never);
  vi.resetModules();
  await import("./notice");
  await vi.waitFor(() => expect(document.querySelector(".copied-notice strong")?.textContent).toContain("Pasted in APA"));
  const change = document.querySelector<HTMLButtonElement>(".copied-notice button")!;
  expect(change.textContent).toBe("Change");
  change.click();
  expect(native.changeSuggestedFormat).toHaveBeenCalledOnce();
  const show = native.listen.mock.calls.find(([name]) => name === "notice:show")![1] as (event: { payload: unknown }) => void;
  show({ payload: { delivered: { outcome: "pasted", app: "Word" }, text: "(García, 2021)", language: "en-US" } });
  expect(document.querySelector(".copied-notice button")).toBeNull();
});

test("the notice speaks the language its message carries, and mirrors for Arabic", async () => {
  document.body.innerHTML = '<div id="notice"></div>';
  native.copiedNotice.mockResolvedValueOnce({ delivered: { outcome: "copied", reason: "paste-unavailable" }, text: "(Aster, 2026)", more: 2, language: "ar" } as never);
  vi.resetModules();
  const { setInterfaceLanguage } = await import("./i18n");
  await import("./notice");
  try {
    await vi.waitFor(() => expect(document.documentElement).toMatchObject({ lang: "ar", dir: "rtl" }));
    expect(document.querySelector(".copy-receipt-more")?.textContent).toBe("+⁨2⁩ أخرى");
  } finally { setInterfaceLanguage("en-US"); }
});
