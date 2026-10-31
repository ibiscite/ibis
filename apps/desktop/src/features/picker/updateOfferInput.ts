// One idle timer, owned by the latest interaction. Only an empty, focused,
// visible picker can return its native activity ticket; no text crosses IPC.
export const UPDATE_OFFER_IDLE_MS = 1500;
export function monitorUpdateOfferInput(
  target: EventTarget,
  report: (ticket: number | null) => Promise<number | null>,
  blocked: () => boolean,
  schedule: (run: () => void) => ReturnType<typeof setTimeout> = (run) => setTimeout(run, UPDATE_OFFER_IDLE_MS),
  cancel: (timer: ReturnType<typeof setTimeout>) => void = clearTimeout,
) {
  let active = true, revision = 0, composing = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const held = new Set<string>();
  const activity = () => {
    if (!active) return;
    const owner = ++revision;
    if (timer !== undefined) cancel(timer);
    timer = undefined;
    const ticket = report(null).catch(() => null);
    if (blocked() || composing || held.size) return;
    timer = schedule(() => {
      if (!active || owner !== revision) return;
      timer = undefined;
      void ticket.then((value) => {
        if (active && owner === revision && !blocked() && !composing && !held.size && value !== null) {
          return report(value).catch(() => null);
        }
      });
    });
  };
  const onKey = (event: Event) => {
    const key = event as KeyboardEvent;
    if (event.type === 'keydown') held.add(key.code);
    else held.delete(key.code);
    activity();
  };
  const onComposition = (event: Event) => { composing = event.type === 'compositionstart'; activity(); };
  const onBlur = (event: Event) => { if (event.target === target) held.clear(); activity(); };
  const onPointer = (event: Event) => {
    if (event.type === 'pointerdown') held.add('pointer');
    else held.delete('pointer');
    activity();
  };
  const listeners: [string, EventListener][] = [
    ['keydown', onKey], ['keyup', onKey], ['beforeinput', activity], ['input', activity],
    ['pointerdown', onPointer], ['pointerup', onPointer], ['pointercancel', onPointer], ['focus', activity], ['blur', onBlur],
    ['compositionstart', onComposition], ['compositionend', onComposition],
  ];
  for (const [type, handler] of listeners) target.addEventListener(type, handler, true);
  activity();
  return { activity, stop() {
    if (!active) return;
    active = false; revision++;
    if (timer !== undefined) cancel(timer);
    for (const [type, handler] of listeners) target.removeEventListener(type, handler, true);
    void report(null).catch(() => null);
  } };
}
