import { pickerText } from "../../messages";
import { isMac } from "../../platform";
import { UndoNotice } from "../../ui/UndoNotice";
import { KeysDialog } from "./KeysDialog";
import type { usePickerSession } from "./usePickerSession";
import type { usePickerMenus } from "./usePickerMenus";
import type { KeyBinding } from "../../keys";

export function PickerOverlays({ state, send, menus, bindings }: {
  state: ReturnType<typeof usePickerSession>["state"];
  send: ReturnType<typeof usePickerSession>["send"];
  menus: ReturnType<typeof usePickerMenus>;
  bindings: readonly KeyBinding[];
}) {
  const { keysOpen, closeKeys, windowUndo, input } = menus;
  return <>
    {keysOpen && <KeysDialog bindings={bindings} isMac={isMac} onClose={closeKeys} />}
    {(state.removed || windowUndo.ticket !== null) && <div className="undo-notices" role="region" aria-label={pickerText.undo}>
      {state.removed && <UndoNotice key={`reference-${state.removed.token}`} message="Removed from citation." paused={state.hiddenAt !== null} fallbackFocusRef={input}
        onExpire={() => send({ type: "expire-removal", removal: state.removed! })}
        onUndo={async () => { send({ type: "undo-reference", removal: state.removed! }); }} />}
      {windowUndo.ticket !== null && <UndoNotice key={`window-${windowUndo.ticket}`} message={pickerText.windowReset} onUndo={windowUndo.undo} onExpire={windowUndo.expire} paused={state.hiddenAt !== null} fallbackFocusRef={input} />}
    </div>}
  </>;
}
