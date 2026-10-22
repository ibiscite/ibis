import { expect, test, vi } from "vitest";

const { readFileSync } = await vi.importActual<{
  readFileSync: (path: URL, encoding: string) => string;
}>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
const tokens = readFileSync(new FileURL("./tokens.css", import.meta.url), "utf8");

// These are physical viewport and target roles; text scaling must not replace
// the 460 px count-only boundary or the 44 px coarse-pointer target at 16 px.
test("shared chip and touch roles retain their agreed physical values", () => {
  expect(tokens).toMatch(/--chip-count-only-width:\s*460px/);
  expect(tokens).toMatch(/--touch-target:\s*2\.75rem/);
});
