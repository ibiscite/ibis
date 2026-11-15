// Decision 0047's going-back table, walked row by row through the one view
// stack (ui/layers): each way back, on each system, does what the table says
// where it lands, once per press, and never during a composition.
import { useState, type ReactNode } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Inspector } from "./Inspector";
import { Palette } from "./Palette";
import { Sheet } from "./Sheet";
import { useLayer } from "./layers";

afterEach(() => { cleanup(); delete document.documentElement.dataset.system; });

type Row = "palette" | "inspector" | "sheet" | "search";
type Way = "escape" | "erase" | "back" | "mouse";
/** Where each way back goes back (true) or does nothing (false). */
const TABLE: Record<Row, Record<Way, boolean>> = {
  search: { escape: true, erase: false, back: false, mouse: false },
  palette: { escape: true, erase: true, back: true, mouse: true },
  inspector: { escape: true, erase: false, back: true, mouse: true },
  sheet: { escape: true, erase: false, back: true, mouse: true },
};

function Popup({ close }: { close: () => void }) {
  useLayer("menu", true, close);
  return <div role="menu">Pop-up</div>;
}
function Search({ close, children }: { close: () => void; children: ReactNode }) {
  useLayer("search", true, close);
  return <>{children}</>;
}
function open(row: Row) {
  const close = vi.fn();
  function Example() {
    const [popup, setPopup] = useState(false);
    const children = <><input aria-label="Draft" /><button onClick={() => setPopup(true)}>Pop-up</button>{popup && <Popup close={() => setPopup(false)} />}</>;
    return row === "palette"
      ? <Palette id="back" title="Formats" placeholder="Search formats" sections={[]} onAction={vi.fn()} onClose={close} actionLabel={() => "Use"} empty="No formats" />
      : row === "sheet" ? <Sheet title="Form" closeLabel="Back" onClose={close}>{children}</Sheet>
        : row === "search" ? <Search close={close}>{children}</Search>
          : <Inspector label="Details" heading="Reference" closeLabel="Close" onClose={close}>{children}</Inspector>;
  }
  render(<Example />);
  const field = screen.getByRole(row === "palette" ? "combobox" : "textbox");
  field.focus();
  return { close, field };
}

const platforms = ["macos", "windows", "linux"] as const;
const press = (field: HTMLElement, way: Way, platform: (typeof platforms)[number], extra: object = {}) => way === "mouse"
  ? fireEvent.mouseDown(field, { button: 3 })
  : fireEvent.keyDown(field, way === "escape" ? { key: "Escape", ...extra } : way === "erase" ? { key: "Backspace", ...extra }
    : platform === "macos" ? { key: "[", metaKey: true, ...extra } : { key: "ArrowLeft", altKey: true, ...extra });

const cases = platforms.flatMap((platform) => (Object.keys(TABLE) as Row[]).flatMap((row) =>
  (["escape", "erase", "back", "mouse"] as const).map((way) => ({ platform, row, way, goes: TABLE[row][way] }))));

test.each(cases)("$platform › $row › $way goes back: $goes", ({ platform, row, way, goes }) => {
  document.documentElement.dataset.system = platform;
  const { close, field } = open(row);
  // An input method owns every key until its composition ends.
  fireEvent.compositionStart(field);
  press(field, way, platform);
  expect(close).not.toHaveBeenCalled();
  fireEvent.compositionEnd(field);
  // A held key never walks back: it must be released and pressed again.
  if (way !== "mouse") press(field, way, platform, { repeat: true });
  expect(close).not.toHaveBeenCalled();
  const unused = press(field, way, platform);
  expect(close).toHaveBeenCalledTimes(goes ? 1 : 0);
  // A way back that acted is spent: nothing under it sees the key.
  if (goes) expect(unused).toBe(false);
});

test.each<Row>(["palette", "inspector", "sheet"])("%s: Esc goes back even with text; Backspace with text only edits", (row) => {
  const { close, field } = open(row);
  fireEvent.change(field, { target: { value: "Draft" } });
  fireEvent.keyDown(field, { key: "Backspace" });
  expect(close).not.toHaveBeenCalled();
  fireEvent.keyDown(field, { key: "Escape" });
  expect(close).toHaveBeenCalledOnce();
});

test("the other system's Back key and other mouse buttons do nothing", () => {
  document.documentElement.dataset.system = "macos";
  const { close, field } = open("sheet");
  fireEvent.keyDown(field, { key: "ArrowLeft", altKey: true });
  fireEvent.mouseDown(field, { button: 4 });
  expect(close).not.toHaveBeenCalled();
});

test.each<Row>(["sheet", "inspector"])("%s: an open pop-up gets Esc before the view, and keeps Back and empty Backspace", (row) => {
  const { close, field } = open(row);
  fireEvent.click(screen.getByRole("button", { name: "Pop-up" }));
  fireEvent.keyDown(field, { key: "Backspace" });
  fireEvent.keyDown(field, { key: "ArrowLeft", altKey: true });
  fireEvent.mouseDown(field, { button: 3 });
  expect(close).not.toHaveBeenCalled();
  expect(screen.getByRole("menu")).toBeTruthy();
  fireEvent.keyDown(field, { key: "Escape" });
  expect(screen.queryByRole("menu")).toBeNull();
  expect(close).not.toHaveBeenCalled();
  fireEvent.keyDown(field, { key: "Escape" });
  expect(close).toHaveBeenCalledOnce();
});

test("a palette leaves Backspace and Back with its open pop-up", () => {
  const { close, field } = open("palette");
  const popupClose = vi.fn();
  render(<Popup close={popupClose} />);
  fireEvent.keyDown(field, { key: "Backspace" });
  fireEvent.keyDown(field, { key: "ArrowLeft", altKey: true });
  fireEvent.mouseDown(field, { button: 3 });
  expect(close).not.toHaveBeenCalled();
  fireEvent.keyDown(field, { key: "Escape" });
  expect(popupClose).toHaveBeenCalledOnce();
  expect(close).not.toHaveBeenCalled();
});

test("while a view is open the window shows only it, and what is under it shows again as it leaves", () => {
  const { close, field } = open("sheet");
  expect(document.documentElement.dataset.view).toBe("");
  close.mockImplementation(() => expect(document.documentElement.dataset.view).toBeUndefined());
  fireEvent.keyDown(field, { key: "Escape" });
  expect(close).toHaveBeenCalledOnce();
});
