import { useRef, useState } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { PickerHeader } from "./PickerHeader";

afterEach(cleanup);

function Header({ changed }: { changed: (query: string) => void }) {
  const input = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  return <PickerHeader inputRef={input} query={query} onQueryChange={(next) => { changed(next); setQuery(next); }}
    expanded activeDescendant="result-0" format="pandoc" showFormat={false}
    styles={false} onFormatChange={() => undefined} />;
}

test.each([
  ["a\nb", "a\tb"],
  ["\\cite{a,\nb}", "\\cite{a,\tb}"],
  ["\\autocites[see][p. 4]{a}\r\n[p. 7]{b}", "\\autocites[see][p. 4]{a}\t[p. 7]{b}"],
  ['#cite(\nlabel("日本🌙"),\n<b>)', '#cite(\tlabel("日本🌙"),\t<b>)'],
  ["a\tb", "a\tb"],
])("multiline paste keeps citation whitespace: %j", (pasted, expected) => {
  const changed = vi.fn();
  render(<Header changed={changed} />);
  const input = screen.getByRole("combobox") as HTMLInputElement;
  fireEvent.change(input, { target: { value: "before replace after" } });
  input.setSelectionRange(7, 14);
  changed.mockClear();
  fireEvent.paste(input, { clipboardData: { getData: () => pasted } });
  expect(changed.mock.calls).toEqual([[`before ${expected} after`]]);
  expect(input.value).toBe(`before ${expected} after`);
});

test("a filled search keeps its query and offers no clear button", () => {
  render(<Header changed={vi.fn()} />);
  const input = screen.getByRole("combobox", { name: "Search references" }) as HTMLInputElement;
  fireEvent.change(input, { target: { value: "研究 العربية" } });
  expect(input.value).toBe("研究 العربية");
  expect(input.getAttribute("aria-activedescendant")).toBe("result-0");
  expect(screen.queryByRole("button", { name: "Clear Search" })).toBeNull();
});
