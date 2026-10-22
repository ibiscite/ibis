import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useRef, useState } from "react";
import axe from "axe-core";
import { ChipGroup, type ChipItem } from "./ChipGroup";

const items: ChipItem[] = [
  { id: "one\0shared", label: "García 2021", textValue: "García 2021, one", removeLabel: "Remove García from one" },
  { id: "two\0shared", label: "García 2021", textValue: "García 2021, two", removeLabel: "Remove García from two" },
  { id: 'three\0quoted"\n]', label: "العربية 2020", textValue: "العربية 2020, three", removeLabel: "Remove العربية from three" },
];
afterEach(async () => {
  cleanup();
  await act(async () => { await new Promise(requestAnimationFrame); });
  vi.restoreAllMocks(); vi.unstubAllGlobals();
  delete document.documentElement.dataset.platform;
});

function mount(locale = "en-US", unmountEmpty = false) {
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  const removed = vi.fn();
  const parentKeys = vi.fn();
  function Preview() {
    const [chosen, setChosen] = useState(items);
    const fallback = useRef<HTMLInputElement>(null);
    return <main onKeyDown={parentKeys}><input ref={fallback} aria-label="Search" />
      {(!unmountEmpty || chosen.length > 0) && <ChipGroup label="Chosen labels" items={chosen} locale={locale} fallbackFocusRef={fallback}
        onRemove={(keys) => { removed([...keys]); setChosen((previous) => previous.filter((item) => !keys.has(item.id))); }} />}
      <button>After chips</button>
    </main>;
  }
  render(<Preview />);
  return { removed, parentKeys, search: screen.getByRole("textbox", { name: "Search" }) };
}

const row = (index: number) => screen.getByRole("row", { name: items[index]!.textValue });
const press = (key: string, extra: Record<string, unknown> = {}) => fireEvent.keyDown(document.activeElement!, { key, ...extra });

test.each(["macos", "windows", "linux"])("%s chips navigate without changing membership or order, with named removal and no axe violations", async (platform) => {
  document.documentElement.dataset.platform = platform;
  const { removed } = mount();
  act(() => row(0).focus());
  press("ArrowRight");
  expect(document.activeElement).toBe(row(1));
  press("ArrowLeft");
  expect(document.activeElement).toBe(row(0));
  expect(screen.getAllByRole("row").map((element) => element.getAttribute("aria-label"))).toEqual(items.map((item) => item.textValue));
  for (const item of items) expect(screen.getByRole("button", { name: item.removeLabel })).toBeTruthy();
  expect(removed).not.toHaveBeenCalled();
  expect((await axe.run(document.body, { rules: { "color-contrast": { enabled: false } } })).violations).toEqual([]);
});

test("RTL context mirrors arrow navigation and the rendered direction together", () => {
  const { removed } = mount("ar-AE");
  expect(screen.getByRole("grid", { name: "Chosen labels" }).closest("[dir]")?.getAttribute("dir")).toBe("rtl");
  act(() => row(0).focus());
  press("ArrowLeft"); expect(document.activeElement).toBe(row(1));
  press("ArrowRight"); expect(document.activeElement).toBe(row(0));
  expect(removed).not.toHaveBeenCalled();
});

test("middle and end removal preserve full identities and move focus next, then previous", async () => {
  const { removed } = mount();
  act(() => row(1).focus());
  press("Delete");
  expect(removed).toHaveBeenLastCalledWith([items[1]!.id]);
  await waitFor(() => expect(document.activeElement).toBe(row(2)));
  press("Backspace");
  expect(removed).toHaveBeenLastCalledWith([items[2]!.id]);
  await waitFor(() => expect(document.activeElement).toBe(row(0)));
  expect(screen.getAllByRole("row")).toHaveLength(1);
});

test.each([false, true])("final removal returns focus to the fallback (group unmounts: %s)", async (unmountEmpty) => {
  const { removed, search } = mount("en-US", unmountEmpty);
  act(() => row(0).focus());
  for (let i = 0; i < items.length; i++) press("Delete");
  await waitFor(() => expect(document.activeElement).toBe(search));
  expect(removed.mock.calls.map(([keys]) => keys)).toEqual(items.map((item) => [item.id]));
  expect(screen.queryByRole("row")).toBeNull();
});

test("held Delete removes the newly focused chip once per event", async () => {
  const { removed } = mount();
  act(() => row(0).focus()); press("Delete");
  await waitFor(() => expect(document.activeElement).toBe(row(1)));
  press("Delete", { repeat: true });
  expect(removed.mock.calls.map(([keys]) => keys)).toEqual([[items[0]!.id], [items[1]!.id]]);
  expect(screen.getAllByRole("row")).toHaveLength(1);
});

test("composition and 229 key events neither remove nor reach parent shortcuts", () => {
  const { removed, parentKeys } = mount();
  act(() => row(0).focus());
  fireEvent.compositionStart(row(0));
  press("Delete", { isComposing: true }); press("Backspace"); press("Escape");
  fireEvent.compositionEnd(row(0));
  press("Delete", { keyCode: 229 }); press("Delete", { isComposing: true });
  expect(removed).not.toHaveBeenCalled(); expect(parentKeys).not.toHaveBeenCalled();
  press("Delete"); expect(removed).toHaveBeenCalledExactlyOnceWith([items[0]!.id]);
});

test("the remove button retains its source-specific name and keyboard activation", async () => {
  const { removed } = mount();
  const remove = screen.getByRole("button", { name: items[1]!.removeLabel });
  act(() => remove.focus());
  fireEvent.keyDown(remove, { key: "Enter" });
  fireEvent.keyDown(remove, { key: "Enter", repeat: true });
  fireEvent.keyUp(remove, { key: "Enter" });
  await waitFor(() => expect(removed).toHaveBeenCalledExactlyOnceWith([items[1]!.id]));
});

test("50 controlled opaque IDs retain caller order and the focused identity after reordering", async () => {
  const many = Array.from({ length: 50 }, (_, i) => ({ ...items[i % 3]!, id: `source${i}\0shared`, textValue: `Label ${i}` }));
  const fallback = { current: null };
  const removed = vi.fn();
  const view = render(<ChipGroup label="Chosen" items={many} onRemove={removed} fallbackFocusRef={fallback} />);
  act(() => screen.getByRole("row", { name: "Label 24" }).focus());
  view.rerender(<ChipGroup label="Chosen" items={[...many].reverse()} onRemove={removed} fallbackFocusRef={fallback} />);
  expect(screen.getAllByRole("row").map((element) => element.getAttribute("aria-label"))).toEqual(many.map((item) => item.textValue).reverse());
  expect(document.activeElement).toBe(screen.getByRole("row", { name: "Label 24" }));
  press("Delete"); expect(removed).toHaveBeenCalledExactlyOnceWith(new Set([many[24]!.id]));
});

test.each([false, true])("final removal leaves a newly focused control alone (unmounts: %s)", async (unmountEmpty) => {
  function Preview() {
    const [chosen, setChosen] = useState(items.slice(0, 1));
    const fallback = useRef<HTMLInputElement>(null);
    const next = useRef<HTMLButtonElement>(null);
    return <><input aria-label="Fallback" ref={fallback} />
      {(!unmountEmpty || chosen.length > 0) && <ChipGroup label="Chosen" items={chosen} fallbackFocusRef={fallback}
        onRemove={() => { setChosen([]); next.current?.focus(); }} />}
      <button ref={next}>New destination</button></>;
  }
  render(<Preview />);
  act(() => row(0).focus()); press("Delete");
  await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("button", { name: "New destination" })));
});
