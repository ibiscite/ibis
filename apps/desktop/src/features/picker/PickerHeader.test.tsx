import { useRef, useState } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { PickerHeader } from "./PickerHeader";

afterEach(cleanup);

function Header({ changed }: { changed: (query: string) => void }) {
  const input = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  return <PickerHeader inputRef={input} query={query} onQueryChange={(next) => { changed(next); setQuery(next); }}
    onSearchKeyDown={() => undefined} expanded activeDescendant="result-0" format="pandoc" showFormat={false}
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
