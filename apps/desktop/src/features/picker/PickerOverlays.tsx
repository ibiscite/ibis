import type { UpdateProgress } from "../../api";
import { useInterfaceMessages } from "../../i18n";
import { pickerText } from "../../messages";
import { isMac } from "../../platform";
import { NoticeSlot, QuietNotice } from "../../ui/NoticeSlot";
import { UndoNotice } from "../../ui/UndoNotice";
import { KeysDialog } from "./KeysDialog";
import { UpdateNotice } from "./Notices";
import { tileName } from "./rows";
import type { usePickerSession } from "./usePickerSession";
import type { usePickerMenus } from "./usePickerMenus";
import type { KeyBinding } from "../../keys";

export function PickerOverlays({ menus, bindings }: { menus: ReturnType<typeof usePickerMenus>; bindings: readonly KeyBinding[] }) {
  return menus.keysOpen ? <KeysDialog bindings={bindings} isMac={isMac} onClose={menus.closeKeys} /> : null;
}

/** What passes, in the footer's status area; the library status returns after it. */
export function PickerNotices({ state, send, menus, updateProgress, remark, passive }: {
  state: ReturnType<typeof usePickerSession>["state"];
  send: ReturnType<typeof usePickerSession>["send"];
  menus: ReturnType<typeof usePickerMenus>;
  updateProgress: UpdateProgress | null;
  remark: string | null;
  passive: boolean;
}) {
  const { text } = useInterfaceMessages();
  const removed = state.removed?.reference;
  const reference = removed ? tileName(removed) : "";
  const { windowUndo, input } = menus;
  return <NoticeSlot>
    {updateProgress && <UpdateNotice progress={updateProgress} />}
    {remark && <QuietNotice passive={passive}>{remark}</QuietNotice>}
    {state.removed && <UndoNotice key={`reference-${state.removed.token}`} message={text("undo-removed", { reference })} action={text("undo-remove", { reference })} paused={state.hiddenAt !== null} fallbackFocusRef={input}
        onExpire={() => send({ type: "expire-removal", removal: state.removed! })}
        onUndo={async () => { send({ type: "undo-reference", removal: state.removed! }); }} />}
    {windowUndo.ticket !== null && <UndoNotice key={`window-${windowUndo.ticket}`} message={pickerText.windowReset} action={text("undo-reset-window")} onUndo={windowUndo.undo} onExpire={windowUndo.expire} paused={state.hiddenAt !== null} fallbackFocusRef={input} />}
  </NoticeSlot>;
}
