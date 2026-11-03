import { useInterfaceMessages } from "../../i18n";
import { pickerText } from "../../messages";
import { isMac } from "../../platform";
import { NoticeDock, QuietNotice } from "../../ui/NoticeDock";
import { UndoNotice } from "../../ui/UndoNotice";
import { KeysDialog } from "./KeysDialog";
import type { usePickerSession } from "./usePickerSession";
import type { usePickerMenus } from "./usePickerMenus";
import type { KeyBinding } from "../../keys";

export function PickerOverlays({ menus, bindings }: { menus: ReturnType<typeof usePickerMenus>; bindings: readonly KeyBinding[] }) {
  return menus.keysOpen ? <KeysDialog bindings={bindings} isMac={isMac} onClose={menus.closeKeys} /> : null;
}

/** Undo offers and passing remarks, in the layout above the shelf. */
export function PickerNoticeDock({ state, send, menus, remark, passive }: {
  state: ReturnType<typeof usePickerSession>["state"];
  send: ReturnType<typeof usePickerSession>["send"];
  menus: ReturnType<typeof usePickerMenus>;
  remark: string | null;
  passive: boolean;
}) {
  const { text } = useInterfaceMessages();
  const removed = state.removed?.reference;
  const reference = removed ? [removed.authors || removed.first_author || removed.key, removed.year].filter((value) => value !== null && value !== undefined).join(", ") : "";
  const { windowUndo, input } = menus;
  return <NoticeDock>
    {state.removed && <UndoNotice key={`reference-${state.removed.token}`} message={text("undo-removed", { reference })} action={text("undo-remove", { reference })} paused={state.hiddenAt !== null} fallbackFocusRef={input}
        onExpire={() => send({ type: "expire-removal", removal: state.removed! })}
        onUndo={async () => { send({ type: "undo-reference", removal: state.removed! }); }} />}
    {windowUndo.ticket !== null && <UndoNotice key={`window-${windowUndo.ticket}`} message={pickerText.windowReset} action={text("undo-reset-window")} onUndo={windowUndo.undo} onExpire={windowUndo.expire} paused={state.hiddenAt !== null} fallbackFocusRef={input} />}
    {remark && <QuietNotice passive={passive}>{remark}</QuietNotice>}
  </NoticeDock>;
}
