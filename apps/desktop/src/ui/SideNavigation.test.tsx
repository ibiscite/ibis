import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { SideNavigation } from "./SideNavigation";

afterEach(() => { cleanup(); delete document.documentElement.dataset.platform; });

const options = [
  { value: "general", label: "General settings and application preferences", tile: <svg aria-hidden="true"><path /></svg> },
  { value: "shortcuts", label: "Keyboard Shortcuts and navigation preferences", tile: <svg aria-hidden="true"><path /></svg> },
  { value: "libraries", label: "Libraries", tile: <svg aria-hidden="true"><path /></svg> },
  { value: "about", label: "About", tile: <svg aria-hidden="true"><path /></svg> },
];

function mount() {
  const changed = vi.fn();
  function Example() {
    const [value, setValue] = useState("general");
    return <SideNavigation label="Pages" value={value} options={options} onChange={(next) => { changed(next); setValue(next); }} />;
  }
  render(<Example />);
  return changed;
}

test.each(["macos", "windows", "linux"])("%s navigation has one Tab stop and follows arrows and Home/End without toggling off", (platform) => {
  document.documentElement.dataset.platform = platform;
  mount();
  const list = screen.getByRole("listbox", { name: "Pages" });
  const items = screen.getAllByRole("option");
  expect(items.map((item) => item.textContent)).toEqual(options.map((option) => option.label));
  expect([list, ...items].filter((item) => item.tabIndex === 0)).toHaveLength(1);
  act(() => items[0]!.focus());
  for (const [key, index] of [["ArrowDown", 1], ["End", 3], ["ArrowUp", 2], ["Home", 0]] as const) {
    fireEvent.keyDown(document.activeElement!, { key });
    expect(document.activeElement).toBe(items[index]);
    expect(screen.getAllByRole("option", { selected: true })).toEqual([items[index]]);
    expect(items.filter((item) => item.getAttribute("aria-current") === "page")).toEqual([items[index]]);
    expect([list, ...items].filter((item) => item.tabIndex === 0)).toEqual([items[index]]);
  }
  fireEvent.click(items[0]!);
  expect(screen.getAllByRole("option", { selected: true })).toEqual([items[0]]);
});

test("pointer selection and typeahead followed by Enter choose the current page", () => {
  const changed = mount();
  const about = screen.getByRole("option", { name: "About" });
  fireEvent.click(about);
  expect(changed).toHaveBeenLastCalledWith("about");
  expect(about.getAttribute("aria-current")).toBe("page");
  act(() => about.focus());
  fireEvent.keyDown(about, { key: "l" });
  const libraries = screen.getByRole("option", { name: "Libraries" });
  expect(document.activeElement).toBe(libraries);
  fireEvent.keyDown(libraries, { key: "Enter", code: "Enter" });
  fireEvent.keyUp(libraries, { key: "Enter", code: "Enter" });
  expect(libraries.getAttribute("aria-selected")).toBe("true");
  expect(changed).toHaveBeenLastCalledWith("libraries");
});

test("a single mouse click notifies once when selecting or reselecting a page", () => {
  const changed = mount();
  const click = (name: string, value: string) => {
    const item = screen.getByRole("option", { name });
    changed.mockClear();
    fireEvent.pointerDown(item, { button: 0, buttons: 1, pointerType: "mouse", pointerId: 1 });
    fireEvent.pointerUp(item, { button: 0, buttons: 0, pointerType: "mouse", pointerId: 1 });
    fireEvent.click(item, { button: 0, detail: 1 });
    expect(changed).toHaveBeenCalledExactlyOnceWith(value);
    expect(item.getAttribute("aria-selected")).toBe("true");
    expect(item.getAttribute("aria-current")).toBe("page");
  };
  click(options[0]!.label, "general");
  click("About", "about");
  click("About", "about");
});
