import { expect, test } from "vitest";
import { previewOf } from "./outputs";
import { testStates } from "../test-fixtures";

test("the format view model shows engine output and never an engine lookup ID", () => {
  const reference = testStates[1]!.results[0]!;
  expect(JSON.stringify(reference.id)).toContain("SENTINEL-ID");
  const prepared = { text: `[see @${reference.key}, p. 33]` };
  const shown = [previewOf("pandoc", prepared), previewOf("apa", null)];
  expect(shown[0]).toEqual({ text: prepared.text, more: 0, example: false });
  expect(shown[1]?.example).toBe(true);
  expect(shown.map((preview) => preview?.text).join(" ")).not.toContain("SENTINEL-ID");
});
