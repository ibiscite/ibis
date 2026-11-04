import { useState } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render } from "@testing-library/react";
import { LAYER_ORDER, closeNearestLayer, closeOverlays, focusLost, useKeyOwner, useLayer, useNearestLayer, type LayerClose, type LayerKind } from "./layers";

afterEach(cleanup);

function Layer({ kind, open = true, close }: { kind: LayerKind; open?: boolean; close: LayerClose }) {
  useLayer(kind, open, close);
  return null;
}
function Nearest() {
  return <output>{useNearestLayer() ?? "none"}</output>;
}

test("Esc closes the nearest kind whatever order the layers opened in", () => {
  const closed: LayerKind[] = [];
  const opened = ["window", "chooser", "sheet", "search", "panel", "menu"] as const;
  render(<>{opened.map((kind) => <Layer key={kind} kind={kind} close={() => { closed.push(kind); }} />)}</>);
  for (let step = 0; step < LAYER_ORDER.length; step += 1) closeNearestLayer();
  // Nothing was closed for real, so the nearest stays; each press asks it again.
  expect(new Set(closed)).toEqual(new Set(["sheet"]));
  cleanup();
  const steps: LayerKind[] = [];
  function Stack() {
    const [open, setOpen] = useState<readonly LayerKind[]>(LAYER_ORDER);
    return <>{[...LAYER_ORDER].reverse().map((kind) => <Layer key={kind} kind={kind} open={open.includes(kind)}
      close={() => { steps.push(kind); setOpen((current) => current.filter((other) => other !== kind)); }} />)}</>;
  }
  render(<Stack />);
  for (let step = 0; step < LAYER_ORDER.length; step += 1) act(() => { closeNearestLayer(); });
  expect(steps).toEqual([...LAYER_ORDER]);
  expect(closeNearestLayer()).toBe(false);
});

test("the latest layer of one kind is nearest, and a layer may hand its key on", () => {
  const first = vi.fn(), second = vi.fn(() => false);
  render(<><Layer kind="menu" close={first} /><Layer kind="menu" close={second} /></>);
  const event = new KeyboardEvent("keydown", { key: "Escape" });
  expect(closeNearestLayer(event)).toBe(false);
  expect(second).toHaveBeenCalledWith(event);
  expect(first).not.toHaveBeenCalled();
});

test("a command clears overlays and leaves the panel, chooser, search and window alone", () => {
  const closed: LayerKind[] = [];
  render(<>{LAYER_ORDER.map((kind) => <Layer key={kind} kind={kind} close={(event) => { if (event) throw new Error("asked by a key"); closed.push(kind); }} />)}</>);
  closeOverlays();
  expect(closed).toEqual(["sheet", "menu"]);
});

test("Esc reaches a layer before any control can swallow it, and only once", () => {
  const close = vi.fn();
  const swallow = vi.fn((event: React.KeyboardEvent) => event.stopPropagation());
  const { getByRole } = render(<><Layer kind="menu" close={close} /><button onKeyDownCapture={swallow}>Control</button></>);
  const button = getByRole("button");
  button.focus();
  expect(fireEvent.keyDown(button, { key: "Escape" })).toBe(false);
  expect(close).toHaveBeenCalledOnce();
  expect(swallow).not.toHaveBeenCalled();
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

test("a composition cut short by its field leaving cannot keep Esc away", () => {
  const close = vi.fn();
  const { getByRole, rerender } = render(<><Layer kind="menu" close={close} /><input /></>);
  fireEvent.compositionStart(getByRole("textbox"));
  rerender(<><Layer kind="menu" close={close} /></>);
  fireEvent.keyDown(document.body, { key: "Escape" });
  expect(close).toHaveBeenCalledOnce();
});

test("a layer that takes the focused control with it sends the keyboard home, and only then", async () => {
  const home = vi.fn();
  function Window({ open, control }: { open: boolean; control: boolean }) {
    useKeyOwner({ keyDown: () => undefined, home });
    useLayer("chooser", open, () => undefined);
    return control ? <button>Row</button> : null;
  }
  const { getByRole, rerender } = render(<Window open control />);
  getByRole("button").focus();
  rerender(<Window open={false} control={false} />);
  expect(document.activeElement).toBe(document.body);
  await act(async () => {});
  expect(home).toHaveBeenCalledOnce();
  home.mockClear();
  rerender(<Window open control />);
  getByRole("button").focus();
  rerender(<Window open={false} control />);
  await act(async () => {});
  expect(home).not.toHaveBeenCalled();
});

test("focus on nothing, or in something hidden, counts as lost", () => {
  const { getByRole, rerender } = render(<div><button>Control</button></div>);
  expect(focusLost()).toBe(true);
  getByRole("button").focus();
  expect(focusLost()).toBe(false);
  rerender(<div hidden><button>Control</button></div>);
  expect(focusLost()).toBe(true);
});

test("the nearest layer is readable for words such as the footer's Esc hint", () => {
  const { container, rerender } = render(<><Nearest /><Layer kind="search" close={() => undefined} /></>);
  expect(container.textContent).toBe("search");
  rerender(<><Nearest /><Layer kind="search" close={() => undefined} /><Layer kind="sheet" close={() => undefined} /></>);
  expect(container.textContent).toBe("sheet");
  rerender(<><Nearest /></>);
  expect(container.textContent).toBe("none");
});
