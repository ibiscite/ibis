import type * as nativeApi from "../../api";
import { useEffect } from "react";
import { startsWindowDrag } from "../../ui/windowDrag";
import type { usePickerSession } from "./usePickerSession";

/** How close to the window's edge a press starts a native resize. */
const EDGE = 6;
let lastId = 0;

/**
 * Tell the native side while the person moves or resizes the picker. Some
 * Linux compositors take focus for the move grab; the blur then must not
 * hide the window being moved. The move ends with the press, its
 * cancellation, or the first unpressed pointer movement once the compositor
 * gives the pointer back; the native side bounds a move whose end is lost.
 */
export function usePickerMove(api: Pick<typeof nativeApi, "pickerMoving">, session: ReturnType<typeof usePickerSession>["session"]) {
  const { pickerMoving } = api;
  useEffect(() => {
    let moving: { presentation: number; id: number } | null = null;
    const end = () => {
      if (moving === null) return;
      const { presentation, id } = moving;
      moving = null;
      void pickerMoving(presentation, id, false).catch(() => undefined);
    };
    const down = (event: PointerEvent) => {
      const presentation = session.getSnapshot().presentation;
      if (event.button !== 0 || presentation === null) return;
      const edge = event.clientX < EDGE || event.clientY < EDGE || event.clientX > window.innerWidth - EDGE || event.clientY > window.innerHeight - EDGE;
      if (!edge && !startsWindowDrag(event.target)) return;
      end();
      // Numbered in start order, also across reloads, so the native side can
      // refuse whatever arrives late or out of order.
      lastId = Math.max(lastId + 1, Math.floor((performance.timeOrigin + performance.now()) * 1000));
      moving = { presentation, id: lastId };
      void pickerMoving(presentation, lastId, true).catch(() => undefined);
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
