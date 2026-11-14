import { useState } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { Button } from "./Button";
import { IconButton } from "./IconButton";
import { Switch } from "./Switch";
import { Option } from "./Option";
import { PopupButton } from "./PopupButton";
import { Frame, FrameHeading, FrameNotices } from "./Frame";
import { NoticeSlot } from "./NoticeSlot";
import { UndoNotice } from "./UndoNotice";
import { KeysDialog } from "./KeysSheet";
import { Inspector } from "./Inspector";
import { KEY_TABLE } from "../keys";
import { say } from "../i18n";
import { CommandList } from "../features/picker/CommandList";
import { pickerCommands } from "../features/picker/commands";

afterEach(() => { cleanup(); vi.useRealTimers(); });

test("unnamed shared controls are type errors", () => {
  // @ts-expect-error A glyph is not a name.
  const icon = <IconButton icon="close" />;
  // @ts-expect-error Composed content needs a name.
  const button = <Button><span /></Button>;
  // @ts-expect-error A switch cannot omit its label.
  const toggle = <Switch />;
  // @ts-expect-error A list row cannot omit its label.
  const option = <Option icon={null} />;
  // @ts-expect-error A popup cannot omit its label.
  const popup = <PopupButton><span /></PopupButton>;
  expect([icon, button, toggle, option, popup]).toHaveLength(5);
});

test("one notice keeps its live node, Undo registration and remaining time across heading and empty room", async () => {
  vi.useFakeTimers();
  const expire = vi.fn(), undo = vi.fn(async () => {});
  function Picker({ empty }: { empty: boolean }) {
    return <Frame search={<input aria-label={say("picker-search-label")} />} notice={<NoticeSlot>
      <UndoNotice message="Removed Aster 2026" action="Remove Aster 2026" onUndo={undo} onExpire={expire} />
    </NoticeSlot>}>
      {empty ? <FrameNotices /> : <FrameHeading><span>{say("picker-references")}</span></FrameHeading>}
    </Frame>;
  }
  const added: Element[] = [];
  const observe = new MutationObserver((records) => {
    for (const record of records) for (const node of record.addedNodes) if (node instanceof Element) {
      if (node.matches('[role="status"]')) added.push(node);
      added.push(...node.querySelectorAll('[role="status"]'));
    }
  });
  observe.observe(document.body, { subtree: true, childList: true });
  const view = render(<Picker empty={false} />);
  const status = screen.getByRole("status", { name: say("picker-notices") });
  await act(async () => {});
  act(() => vi.advanceTimersByTime(5000));
  view.rerender(<Picker empty />);
  expect(screen.getByRole("status")).toBe(status);
  expect(document.querySelector("main")?.hasAttribute("data-notice-room")).toBe(true);
  view.rerender(<Picker empty={false} />);
  await act(async () => {});
  expect(new Set(added)).toEqual(new Set([status]));
  act(() => vi.advanceTimersByTime(2999));
  expect(expire).not.toHaveBeenCalled();
  act(() => vi.advanceTimersByTime(1));
  expect(expire).toHaveBeenCalledOnce();
  observe.disconnect();
});

test("commands own a heading even with a notice, so the list keeps its fixed slot", () => {
  render(<Frame search={<input />} notice={<NoticeSlot><UndoNotice message="Removed Aster 2026" action="Remove Aster 2026" onUndo={async () => {}} onExpire={() => {}} /></NoticeSlot>}>
    <CommandList commands={pickerCommands({ source: null, changeLast: false })} active={0} hoverAllowed={false} onRun={() => {}} />
  </Frame>);
  expect(document.querySelector(".ui-heading-slot")?.textContent).toBe(say("picker-commands"));
  expect(screen.getByRole("listbox", { name: say("picker-commands") })).toBeTruthy();
});

test.each(["sheet", "inspector"])("%s closes to its opener through the shared layer", async (surface) => {
  function Window() {
    const [open, setOpen] = useState(false);
    return <><Button label={say("key-title")} onPress={() => setOpen(true)} />
      {open && (surface === "sheet" ? <KeysDialog bindings={KEY_TABLE} isMac onClose={() => setOpen(false)} />
        : <Inspector label={say("picker-citation-warnings")} heading={say("picker-citation-warnings")} closeLabel={say("key-close")} readOnly onClose={() => setOpen(false)}><p>Invented warning</p></Inspector>)}
    </>;
  }
  render(<Window />);
  const opener = screen.getByRole("button", { name: say("key-title") });
  act(() => opener.focus());
  fireEvent.click(opener);
  if (surface === "sheet") {
    const dialog = screen.getByRole("dialog");
    fireEvent.keyDown(dialog, { key: "Tab" });
    expect(document.activeElement).toBe(within(dialog).getByRole("group"));
    fireEvent.keyDown(document.activeElement!, { key: "Tab" });
    expect(document.activeElement).toBe(within(dialog).getByRole("button", { name: say("palette-back") }));
    fireEvent.keyDown(document.activeElement!, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(within(dialog).getByRole("group"));
  }
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  await waitFor(() => expect(document.activeElement).toBe(opener));
});
