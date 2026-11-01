import { act, fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { useFileDrag } from "./useFileDrag";

function Target({ subscribe, presentation = 0 }: { subscribe?: (change: (active: boolean) => void) => Promise<() => void>; presentation?: number }) {
  const drag = useFileDrag(subscribe, presentation);
  return <div {...drag} data-testid="content"><span>Choices</span></div>;
}

test("file drag covers the content, survives child crossings and clears on refusal, drop or reopening", () => {
  const view = render(<Target />);
  const area = screen.getByTestId("content");
  fireEvent.dragEnter(area, { dataTransfer: { types: ["Files"] } });
  expect(area.hasAttribute("data-file-drag")).toBe(true);
  fireEvent.dragEnter(screen.getByText("Choices"), { dataTransfer: { types: ["Files"] } });
  fireEvent.dragLeave(screen.getByText("Choices"));
  expect(area.hasAttribute("data-file-drag")).toBe(true);
  fireEvent.drop(area);
  expect(area.hasAttribute("data-file-drag")).toBe(false);
  fireEvent.dragEnter(area, { dataTransfer: { types: ["text/plain"] } });
  expect(area.hasAttribute("data-file-drag")).toBe(false);
  fireEvent.dragEnter(area, { dataTransfer: { types: ["Files"] } });
  view.rerender(<Target presentation={1} />);
  expect(area.hasAttribute("data-file-drag")).toBe(false);
});

test("failed native subscription is quiet; an old subscription cannot reenter after close/reopen", async () => {
  const callbacks: ((active: boolean) => void)[] = [];
  let finish!: (stop: () => void) => void;
  const stop = vi.fn();
  const subscribe = vi.fn((change: (active: boolean) => void) => { callbacks.push(change); return new Promise<() => void>((resolve) => { finish = resolve; }); });
  const view = render(<Target subscribe={subscribe} />);
  view.unmount();
  render(<Target subscribe={async () => { throw new Error("invented I/O failure"); }} />);
  await act(async () => { callbacks[0]!(true); finish(stop); });
  expect(stop).toHaveBeenCalledOnce();
  expect(screen.getByTestId("content").hasAttribute("data-file-drag")).toBe(false);
});

test("a retired native drag cannot mark a reopened presentation", async () => {
  const callbacks: ((active: boolean) => void)[] = [];
  const subscribe = async (change: (active: boolean) => void) => { callbacks.push(change); return () => {}; };
  const view = render(<Target subscribe={subscribe} />);
  act(() => callbacks[0]!(true));
  expect(screen.getByTestId("content").hasAttribute("data-file-drag")).toBe(true);
  view.rerender(<Target subscribe={subscribe} presentation={1} />);
  act(() => callbacks[0]!(true));
  expect(screen.getByTestId("content").hasAttribute("data-file-drag")).toBe(false);
  act(() => callbacks[1]!(true));
  expect(screen.getByTestId("content").hasAttribute("data-file-drag")).toBe(true);
});
