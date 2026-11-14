import { act, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { expect, test, vi } from "vitest";
import { SettingsSearch } from "./SettingsSearch";

test("Esc clears a Settings filter once, keeps typing focus and leaves composition alone", () => {
  const leave = vi.fn();
  function Filter() {
    const [query, setQuery] = useState("研究");
    return <div onKeyDown={leave}><SettingsSearch label="Filter shortcuts" value={query} onChange={setQuery} /></div>;
  }
  render(<Filter />);
  const input = screen.getByRole("searchbox") as HTMLInputElement;
  act(() => input.focus());
  fireEvent.keyDown(input, { key: "Escape", isComposing: true });
  expect(input.value).toBe("研究");
  expect(leave).not.toHaveBeenCalled();
  fireEvent.keyDown(input, { key: "Escape" });
  expect(input.value).toBe("");
  expect(document.activeElement).toBe(input);
  fireEvent.keyDown(input, { key: "Escape", repeat: true });
  expect(leave).not.toHaveBeenCalled();
  fireEvent.keyDown(input, { key: "Escape" });
  expect(leave).toHaveBeenCalledOnce();
});
