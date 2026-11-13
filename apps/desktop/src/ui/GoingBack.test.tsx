import { useState } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Inspector } from "./Inspector";
import { Palette } from "./Palette";
import { Sheet } from "./Sheet";
import { useLayer } from "./layers";

afterEach(() => { cleanup(); delete document.documentElement.dataset.system; });

type Kind = "palette" | "inspector" | "sheet";
function Popup({ close }: { close: () => void }) {
  useLayer("menu", true, close);
  return <div role="menu">Pop-up</div>;
}
function open(kind: Kind) {
  const close = vi.fn();
  function Example() {
    const [popup, setPopup] = useState(false);
    const children = <><input aria-label="Draft" /><button onClick={() => setPopup(true)}>Pop-up</button>{popup && <Popup close={() => setPopup(false)} />}</>;
    return kind === "palette"
      ? <Palette id="back" title="Formats" placeholder="Search formats" sections={[]} onAction={vi.fn()} onClose={close} actionLabel={() => "Use"} empty="No formats" />
      : kind === "sheet" ? <Sheet title="Form" closeLabel="Back" onClose={close}>{children}</Sheet>
        : <Inspector label="Details" heading="Reference" closeLabel="Close" onClose={close}>{children}</Inspector>;
  }
  render(<Example />);
  const field = screen.getByRole(kind === "palette" ? "combobox" : "textbox");
  return { close, field };
}

test.each<Kind>(["palette", "inspector", "sheet"])("%s: Esc goes back even with text; IME owns Esc until composition ends", (kind) => {
  const { close, field } = open(kind);
  fireEvent.change(field, { target: { value: "Draft" } });
  fireEvent.compositionStart(field);
  fireEvent.keyDown(field, { key: "Escape" });
  expect(close).not.toHaveBeenCalled();
  fireEvent.compositionEnd(field);
  fireEvent.keyDown(field, { key: "Escape" });
  expect(close).toHaveBeenCalledOnce();
});

test.each<Kind>(["palette", "inspector", "sheet"])("%s: an empty Backspace only navigates in a palette, after releasing the held key", (kind) => {
  const { close, field } = open(kind);
  fireEvent.change(field, { target: { value: "x" } });
  fireEvent.keyDown(field, { key: "Backspace" });
  fireEvent.change(field, { target: { value: "" } });
  fireEvent.keyDown(field, { key: "Backspace", repeat: true });
  expect(close).not.toHaveBeenCalled();
  fireEvent.keyUp(field, { key: "Backspace" });
  fireEvent.keyDown(field, { key: "Backspace" });
  expect(close).toHaveBeenCalledTimes(kind === "palette" ? 1 : 0);
});

const platforms = ["macos", "windows", "linux"] as const;
for (const kind of ["palette", "inspector", "sheet"] as const) {
  test.each(platforms)(`${kind}: %s Back keys and mouse Back close once and respect composition`, (platform) => {
    document.documentElement.dataset.system = platform;
    const { close, field } = open(kind);
    const back = platform === "macos" ? { key: "[", metaKey: true } : { key: "ArrowLeft", altKey: true };
    const wrong = platform === "macos" ? { key: "ArrowLeft", altKey: true } : { key: "[", metaKey: true };
    fireEvent.keyDown(field, wrong);
    fireEvent.mouseDown(field, { button: 4 });
    expect(close).not.toHaveBeenCalled();
    fireEvent.compositionStart(field);
    fireEvent.keyDown(field, back);
    fireEvent.mouseDown(field, { button: 3 });
    expect(close).not.toHaveBeenCalled();
    fireEvent.compositionEnd(field);
    expect(fireEvent.keyDown(field, back)).toBe(false);
    fireEvent.keyDown(field, { ...back, repeat: true });
    expect(close).toHaveBeenCalledOnce();
    expect(fireEvent.mouseDown(field, { button: 3 })).toBe(false);
    expect(close).toHaveBeenCalledTimes(2);
  });
}

test.each<Kind>(["sheet", "inspector"])("%s: an open pop-up gets Esc before the view, and keeps Back and empty Backspace", (kind) => {
  const { close, field } = open(kind);
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
