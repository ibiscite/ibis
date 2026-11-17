import { useState } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render } from "@testing-library/react";
import { KINDS, goBack, useLayer, type LayerClose, type LayerKind } from "./layers";

afterEach(cleanup);
const ORDER = Object.keys(KINDS) as LayerKind[];

function Layer({ kind, open = true, close }: { kind: LayerKind; open?: boolean; close: LayerClose }) {
  useLayer(kind, open, close);
  return null;
}

test("Esc closes the nearest kind whatever order the layers opened in", () => {
  const closed: LayerKind[] = [];
  const opened = ["window", "chooser", "sheet", "search", "panel", "menu"] as const;
  render(<>{opened.map((kind) => <Layer key={kind} kind={kind} close={() => { closed.push(kind); }} />)}</>);
  for (let step = 0; step < ORDER.length; step += 1) goBack();
  // Nothing was closed for real, so the nearest stays; each press asks it again.
  expect(new Set(closed)).toEqual(new Set(["menu"]));
  cleanup();
  const steps: LayerKind[] = [];
  function Stack() {
    const [open, setOpen] = useState<readonly LayerKind[]>(ORDER);
    return <>{[...ORDER].reverse().map((kind) => <Layer key={kind} kind={kind} open={open.includes(kind)}
      close={() => { steps.push(kind); setOpen((current) => current.filter((other) => other !== kind)); }} />)}</>;
  }
  render(<Stack />);
  for (let step = 0; step < ORDER.length; step += 1) act(() => { goBack(); });
  expect(steps).toEqual(ORDER);
  expect(goBack()).toBe(false);
});

test("a composition keeps Esc, even when only its start was flagged", () => {
  const close = vi.fn();
  const { getByRole } = render(<><Layer kind="menu" close={close} /><input /></>);
  const field = getByRole("textbox");
  field.focus();
  fireEvent.keyDown(field, { key: "Escape", isComposing: true });
  fireEvent.keyDown(field, { key: "Escape", keyCode: 229 });
  fireEvent.compositionStart(field);
  fireEvent.keyDown(field, { key: "Escape" });
  expect(close).not.toHaveBeenCalled();
  fireEvent.compositionEnd(field);
  fireEvent.keyDown(field, { key: "Escape" });
  expect(close).toHaveBeenCalledOnce();
});
