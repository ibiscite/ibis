import { listen } from "@tauri-apps/api/event";
import { commands } from "../bindings";
import { composing } from "../composition";
import { matchesAccelerator } from "../keys";
import { currentPlatform } from "../platform";

type Stop = (() => void) & { pause: (paused: boolean) => void };
type Action = { title: string; run: () => void; paused: boolean };

/** One owner per webview. Notice order survives hide/reopen; native events carry
 * the exact publication ticket, so a delayed menu click cannot undo a new action. */
export class UndoOwner {
  private actions = new Map<symbol, Action>();
  revision = 0;
  constructor(private publish: (title: string | null, revision: number) => void) {}
  private get current() { return [...this.actions.values()].reverse().find((action) => !action.paused); }
  get title() { return this.current?.title ?? null; }
  private changed() { this.revision += 1; try { this.publish(this.title, this.revision); } catch { /* Keyboard undo remains available. */ } }
  register(title: string, run: () => void): Stop {
    const identity = Symbol();
    const action = { title, run, paused: false };
    this.actions.set(identity, action); this.changed();
    const stop = (() => { if (this.actions.delete(identity)) this.changed(); }) as Stop;
    stop.pause = (paused) => { if (this.actions.has(identity) && action.paused !== paused) { action.paused = paused; this.changed(); } };
    return stop;
  }
  invoke(revision = this.revision) {
    const action = this.current;
    if (revision !== this.revision || !action) return false;
    action.run(); return true;
  }
}

const owner = new UndoOwner((title, revision) => {
  if ("__TAURI_INTERNALS__" in window) void commands.setEditUndo(title, revision).catch(() => {});
});

const key = (event: KeyboardEvent) => {
  if (composing(event) || !matchesAccelerator(event, "CmdOrCtrl+Z", currentPlatform() === "macos") || !owner.title) return;
  event.preventDefault(); event.stopImmediatePropagation();
  if (!event.repeat) owner.invoke();
};
let notices = 0;
let disconnect: (() => void) | null = null;

/** One transport per webview, only while at least one notice is mounted. */
export function registerUndo(title: string, run: () => void) {
  if (notices++ === 0) {
    window.addEventListener("keydown", key, true);
    const listening = "__TAURI_INTERNALS__" in window ? listen<{ revision: number }>("edit:undo", ({ payload }) => owner.invoke(payload.revision)) : null;
    disconnect = () => {
      window.removeEventListener("keydown", key, true);
      void listening?.then((unlisten) => unlisten()).catch(() => {});
    };
  }
  const stop = owner.register(title, run);
  return Object.assign(() => {
    stop();
    if (--notices === 0) { disconnect?.(); disconnect = null; }
  }, { pause: stop.pause });
}
