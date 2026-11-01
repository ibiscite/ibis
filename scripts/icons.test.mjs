import assert from "node:assert/strict";
import { test } from "node:test";
import { iconProblems } from "./check.mjs";

const drawing = '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M4 4L16 16M16 4L4 16"/></svg>';

test("the icon guard accepts the shared grid and rejects off-grid, filled and mismatched strokes", () => {
  assert.deepEqual(iconProblems(drawing), []);
  for (const invalid of [
    drawing.replace("<svg ", "<svg transform='translate(20)' "),
    drawing.replace("<svg ", "<svg style='stroke-width:20' "),drawing.replace('viewBox="0 0 20 20"', 'viewBox="0 0 24 24"'),
    drawing.replace('stroke-width="1.5"', 'stroke-width="2"'), drawing.replace('fill="none"', 'fill="currentColor"'),
    drawing.replace('L16 16', 'L19 16'), drawing.replace('stroke-linecap="round"', 'stroke-linecap="square"'),
    drawing.replace('/></svg>', '/><path d="M4 4L5 5"/></svg>'),
    drawing.replace('<path d=', '<path transform="translate(2)" d=')]) {
    assert.ok(iconProblems(invalid).length, invalid);
  }
});
