import { useCallback, useEffect, useRef, useState, type RefObject } from "react";
import { useInterfaceMessages } from "../i18n";
import { UndoNotice } from "./UndoNotice";

/** One reset owns its undo. A later edit retires both pending and offered undo. */
export function useSettingUndo(paused: boolean, fallbackFocusRef: RefObject<HTMLElement | null>) {
  const { text } = useInterfaceMessages();
  const generation = useRef(0);
  const [offer, setOffer] = useState<{ ticket: number; label: string; undo: () => Promise<void> } | null>(null);
  const clear = useCallback(() => { generation.current += 1; setOffer(null); }, []);
  useEffect(() => () => { generation.current += 1; }, []);
  const reset = async (label: string, save: () => Promise<void>, undo: () => Promise<void>) => {
    clear();
    // The save starts through the ordinary edit path, which may also retire undo.
    const saving = save();
    const ticket = generation.current;
    try {
      await saving;
      if (ticket === generation.current) setOffer({ ticket, label, undo });
    } catch { /* The setting's owner presents its save failure beside the control. */ }
  };
  const notice = offer && <UndoNotice key={offer.ticket} paused={paused} fallbackFocusRef={fallbackFocusRef}
    message={text("settings-default-restored", { setting: offer.label })} action={text("settings-undo-default", { setting: offer.label })}
    onUndo={async () => {
      if (offer.ticket !== generation.current) return;
      await offer.undo();
      if (offer.ticket === generation.current) clear();
    }} onExpire={() => { if (offer.ticket === generation.current) clear(); }} />;
  return { clear, reset, notice };
}
