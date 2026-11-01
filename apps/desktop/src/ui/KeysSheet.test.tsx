import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { KeysDialog } from "../features/picker/KeysDialog";
import { KEY_TABLE } from "../keys";

afterEach(cleanup);

test("the keys sheet presents compact groups with shared key caps and leaves out standard menu navigation", () => {
  render(<KeysDialog bindings={KEY_TABLE} isMac onClose={vi.fn()} />);
  const dialog = screen.getByRole("dialog", { name: "Keyboard Shortcuts" });
  const groups = within(dialog).getAllByRole("rowgroup").filter((group) => group.tagName === "TBODY");
  expect(groups.map((group) => within(group).getAllByRole("row")[0]!.textContent)).toEqual(["While searching", "Your library", "Anywhere in Ibis"]);
  expect(dialog.querySelector(".keys-groups")).toBeTruthy();
  expect(dialog.querySelectorAll("kbd").length).toBeGreaterThan(4);
  expect(within(dialog).queryByText("Go to the first or last item")).toBeNull();
});

test("the keys sheet keeps its focus cycle and closes with Escape or its backdrop", () => {
  const onClose = vi.fn();
  const view = render(<KeysDialog bindings={KEY_TABLE} isMac onClose={onClose} />);
  const dialog = screen.getByRole("dialog"), close = within(dialog).getByRole("button", { name: "Close" });
  expect(document.activeElement).toBe(dialog);
  fireEvent.keyDown(dialog, { key: "Tab" });
  expect(document.activeElement).toBe(close);
  fireEvent.keyDown(close, { key: "Tab" });
  expect(document.activeElement).toBe(within(dialog).getByRole("group", { name: "Keyboard Shortcuts" }));
  fireEvent.keyDown(document.activeElement!, { key: "Tab" });
  expect(document.activeElement).toBe(close);
  fireEvent.keyDown(close, { key: "Tab", shiftKey: true });
  expect(document.activeElement).not.toBe(close);
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  expect(onClose).toHaveBeenCalledTimes(1);
  fireEvent.pointerDown(dialog);
  expect(onClose).toHaveBeenCalledTimes(1);
  fireEvent.pointerDown(view.container.firstElementChild!);
  expect(onClose).toHaveBeenCalledTimes(2);
  view.unmount();
  render(<KeysDialog bindings={KEY_TABLE} isMac onClose={onClose} />);
  expect(document.activeElement).toBe(screen.getByRole("dialog"));
});
