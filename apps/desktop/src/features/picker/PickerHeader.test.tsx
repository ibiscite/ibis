import { useRef, useState } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { PickerHeader } from "./PickerHeader";

afterEach(cleanup);

function Header({ changed, pinChanged = () => undefined }: { changed: (query: string) => void; pinChanged?: () => void }) {
  const input = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const [pinned, setPinned] = useState(false);
  return <PickerHeader inputRef={input} query={query} onQueryChange={(next) => { changed(next); setQuery(next); }}
    onSearchKeyDown={() => undefined} expanded activeDescendant="result-0" format="pandoc" showFormat={false}
    pinned={pinned} onTogglePin={() => { setPinned((value) => !value); pinChanged(); input.current?.focus(); }}
    styles={false} onFormatChange={() => undefined} onFormatCommand={() => ({ run: () => undefined, isCurrent: () => true })} />;
}

test.each(["pointer", "Enter", "Space"])("clear acts once with %s and immediately returns editing to Search", (activation) => {
  const changed = vi.fn();
  render(<Header changed={changed} />);
  const input = screen.getByRole("combobox", { name: "Search references" }) as HTMLInputElement;
  expect(input.type).toBe("search");
  expect(screen.queryByRole("button", { name: "Clear search" })).toBeNull();
  fireEvent.change(input, { target: { value: "研究 العربية" } });
  const clear = screen.getByRole("button", { name: "Clear search" });
  expect(clear.getAttribute("data-variant")).toBe("icon");
  expect(clear.hasAttribute("data-tauri-drag-region")).toBe(false);
  clear.focus();
  changed.mockClear();
  if (activation === "pointer") fireEvent.click(clear);
  else {
    const key = activation === "Space" ? " " : "Enter";
    const code = activation === "Space" ? "Space" : "Enter";
    fireEvent.keyDown(clear, { key, code });
    fireEvent.keyDown(clear, { key, code, repeat: true });
    fireEvent.keyUp(clear, { key, code });
  }
  expect(changed.mock.calls).toEqual([[""]]);
  expect(input.value).toBe("");
  expect(document.activeElement).toBe(input);
  expect(screen.queryByRole("button", { name: "Clear search" })).toBeNull();
  fireEvent.change(input, { target: { value: "next" } });
  expect(input.value).toBe("next");
  expect(input.getAttribute("aria-activedescendant")).toBe("result-0");
});

test.each([{ isComposing: true }, { keyCode: 229 }])("composition confirmation cannot activate Clear (%j)", (composition) => {
  const changed = vi.fn();
  render(<Header changed={changed} />);
  const input = screen.getByRole("combobox", { name: "Search references" });
  fireEvent.change(input, { target: { value: "研究" } });
  const clear = screen.getByRole("button", { name: "Clear search" });
  clear.focus();
  changed.mockClear();
  fireEvent.keyDown(clear, { key: "Enter", code: "Enter", ...composition });
  fireEvent.keyUp(clear, { key: "Enter", code: "Enter", ...composition });
  expect(changed).not.toHaveBeenCalled();
  expect((input as HTMLInputElement).value).toBe("研究");
});


test.each(["pointer", "Enter", "Space"])("header pin uses the shared icon toggle once with %s", (activation) => {
  const changed = vi.fn();
  render(<Header changed={vi.fn()} pinChanged={changed} />);
  const pin = screen.getByRole("button", { name: "Keep Window Open" });
  expect(pin.closest("header")).not.toBeNull();
  expect(pin.getAttribute("data-variant")).toBe("icon");
  expect(pin.getAttribute("aria-pressed")).toBe("false");
  pin.focus();
  if (activation === "pointer") fireEvent.click(pin);
  else {
    const key = activation === "Space" ? " " : "Enter", code = activation === "Space" ? "Space" : "Enter";
    fireEvent.keyDown(pin, { key, code }); fireEvent.keyDown(pin, { key, code, repeat: true }); fireEvent.keyUp(pin, { key, code });
  }
  expect(changed).toHaveBeenCalledOnce();
  expect(pin.getAttribute("aria-pressed")).toBe("true");
  expect(pin.querySelector("svg")?.getAttribute("fill")).toBe("currentColor");
  expect(document.activeElement).toBe(screen.getByRole("combobox"));
});
