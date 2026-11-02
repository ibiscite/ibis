import { useEffect } from "react";
import type { PickerApi } from "../../App";
import type { usePickerSession } from "./usePickerSession";

/** How close to the window's edge a press starts a native resize. */
const EDGE = 6;

/** As Tauri's own drag script decides: a direct press on a plain drag region, or anywhere in a deep one. */
function startsDrag(target: Element | null) {
  const own = target?.getAttribute("data-tauri-drag-region");
  return own === "" || own === "true" || own === "deep" || Boolean(target?.closest('[data-tauri-drag-region="deep"]'));
}

/**
 * Tell the native side while the person moves or resizes the picker. Some
 * Linux compositors take focus for the move grab; the blur then must not
 * hide the window being moved. The move ends with the press, its
 * cancellation, or the first unpressed pointer movement once the compositor
 * gives the pointer back; the native side bounds a move whose end is lost.
 */
export function usePickerMove(api: Pick<PickerApi, "pickerMoving">, session: ReturnType<typeof usePickerSession>["session"]) {
  const { pickerMoving } = api;
  useEffect(() => {
    if (!pickerMoving) return;
    // The move's id comes back from its start; its end waits for it, so the
    // end can never reach the native side first.
    let moving: { presentation: number; id: Promise<number | null> } | null = null;
    const end = () => {
      if (moving === null) return;
      const { presentation, id } = moving;
      moving = null;
      void id.then((ends) => ends === null ? null : pickerMoving(presentation, ends)).catch(() => undefined);
    };
    const down = (event: PointerEvent) => {
      const presentation = session.getSnapshot().presentation;
      if (event.button !== 0 || presentation === null) return;
      const target = event.target instanceof Element ? event.target : null;
      const edge = event.clientX < EDGE || event.clientY < EDGE || event.clientX > window.innerWidth - EDGE || event.clientY > window.innerHeight - EDGE;
      if (!edge && !startsDrag(target)) return;
      end();
      moving = { presentation, id: pickerMoving(presentation, null).catch(() => null) };
    };
    const move = (event: PointerEvent) => { if (event.buttons === 0) end(); };
    window.addEventListener("pointerdown", down, true);
    window.addEventListener("pointerup", end, true);
    window.addEventListener("pointercancel", end, true);
    window.addEventListener("pointermove", move, true);
    window.addEventListener("focus", end);
    return () => {
      window.removeEventListener("pointerdown", down, true);
      window.removeEventListener("pointerup", end, true);
      window.removeEventListener("pointercancel", end, true);
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("focus", end);
      end();
    };
  }, [pickerMoving, session]);
}
