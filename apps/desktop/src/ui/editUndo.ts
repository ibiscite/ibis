import { composing } from "../composition";
import { matchesAccelerator } from "../keys";
import { currentPlatform } from "../platform";

type Stop = (() => void) & { pause: (paused: boolean) => void; isNewest: () => boolean };
type Action = { run: () => void; paused: boolean; typed: string | null };
type Field = HTMLInputElement | HTMLTextAreaElement;

const text = (node: EventTarget | Element | null) =>
  node instanceof HTMLInputElement || node instanceof HTMLTextAreaElement ? (node as Field).value : null;

/** The notices of one webview. Cmd+Z takes the newest unpaused one, unless the
 * person has typed since it appeared: then it is their text to undo. */
export class UndoOwner {
  private actions = new Map<symbol, Action>();
  private listeners = new Set<() => void>();
  /** The slot shows the newest offer, so one stays beside the words ⌘Z acts on. */
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  private changed() { for (const listener of this.listeners) listener(); }
  private get current() { return [...this.actions.values()].reverse().find((action) => !action.paused); }
  register(run: () => void, typed: string | null = null): Stop {
    const identity = Symbol();
    const action = { run, paused: false, typed };
    this.actions.set(identity, action);
    const stop = (() => { this.actions.delete(identity); this.changed(); }) as Stop;
    stop.pause = (paused) => { action.paused = paused; };
    stop.isNewest = () => [...this.actions.keys()].at(-1) === identity;
    this.changed();
    return stop;
  }
  takes(field: string | null) {
    const action = this.current;
    return Boolean(action) && (field === null || field === action!.typed);
  }
  invoke() {
    const action = this.current;
    if (!action) return false;
    action.run();
    return true;
  }
}

const owner = new UndoOwner();
export const subscribeUndo = owner.subscribe;

const key = (event: KeyboardEvent) => {
  if (composing(event) || !matchesAccelerator(event, "CmdOrCtrl+Z", currentPlatform() === "macos") || !owner.takes(text(event.target))) return;
  event.preventDefault(); event.stopImmediatePropagation();
  if (!event.repeat) owner.invoke();
};
let notices = 0;

/** One listener per webview, only while at least one notice is mounted. */
export function registerUndo(run: () => void) {
  if (notices++ === 0) window.addEventListener("keydown", key, true);
  const stop = owner.register(run, text(document.activeElement));
  return Object.assign(() => {
    stop();
    if (--notices === 0) window.removeEventListener("keydown", key, true);
  }, { pause: stop.pause, isNewest: stop.isNewest });
}
