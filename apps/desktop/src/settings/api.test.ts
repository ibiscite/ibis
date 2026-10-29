import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { ComponentProps } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { settingsApi } from "./api";
import type { PasteGuide } from "./PasteGuide";

afterEach(clearMocks);

test("a null native drag result fulfills the guide's void callback", async () => {
  const invoke = vi.fn(() => null);
  mockIPC(invoke);
  // Keep the real API assignable to the guide, including when bindings are regenerated.
  const onDrag: ComponentProps<typeof PasteGuide>["onDrag"] = settingsApi.dragPasteApp;
  await expect(onDrag()).resolves.toBeUndefined();
  expect(invoke).toHaveBeenCalledExactlyOnceWith("drag_paste_app", {});
});

test("a native drag refusal still rejects so the guide can show recovery", async () => {
  const refusal = new Error("synthetic drag refusal");
  mockIPC(() => { throw refusal; });
  await expect(settingsApi.dragPasteApp()).rejects.toBe(refusal);
});
