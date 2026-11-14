import assert from "node:assert/strict";
import { test } from "node:test";
import { iconProblems } from "./check-icons.mjs";

const drawing = '<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 2L22 22M22 2L2 22"/></svg>';

test("the icon guard accepts the shared grid and rejects off-grid, filled and mismatched strokes", () => {
  assert.deepEqual(iconProblems(drawing), []);
  for (const invalid of [
    drawing.replace("<svg ", "<svg transform='translate(20)' "),
    drawing.replace("<svg ", "<svg style='stroke-width:20' "), drawing.replace('viewBox="0 0 24 24"', 'viewBox="0 0 20 20"'),
    drawing.replace('stroke-width="2"', 'stroke-width="1.5"'), drawing.replace('fill="none"', 'fill="currentColor"'),
    drawing.replace('L22 22', 'L24 22'), drawing.replace('stroke-linecap="round"', 'stroke-linecap="square"'),
    drawing.replace('/></svg>', '/><path d="M4 4L5 5"/></svg>'),
    drawing.replace('<path d=', '<path transform="translate(2)" d=')]) {
    assert.ok(iconProblems(invalid).length, invalid);
  }
});
