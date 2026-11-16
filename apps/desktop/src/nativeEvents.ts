// The one way the webviews hear from native code. Every event is named once,
// with its payload, in `events`; every subscription goes through `listening`,
// which owns what a hand-written one gets wrong: a handler that runs after
// cleanup, a subscription that arrives after it, and a refusal that escapes.
import { listen } from "@tauri-apps/api/event";
import { useEffect, useEffectEvent } from "react";
import type { NoticeMessage, UpdateProgress } from "./api";
import type { EndpointStatus, LibraryStatus, PreferenceChange, PickerSnapshot, ShortcutStatus } from "./bindings";

export type Stop = () => void;
type Handler = (...args: never[]) => unknown;
/** Something to listen to: an entry of `events`, a window's own event, or a test's fake. */
export type Source<H extends Handler> = (handler: H) => Promise<Stop>;

function on<T = null>(name: string): Source<(payload: T) => void> {
  return (handler) => listen<T>(name, ({ payload }) => handler(payload));
}

/** Every native event either window hears, by its one name and payload. */
export const events = {
  /** Shown by the global shortcut or a second launch, with its presentation. */
  onPickerShown: on<number>("picker:shown"),
  /** One complete observation, rendered before native code reveals the window. */
  onPickerPreparing: on<PickerSnapshot>("picker:preparing"),
  /** Hidden, by any route. */
  onPickerHidden: on("picker:hidden"),
  /** Search with Ibis has left text for the picker to collect. */
  onSearchRequested: on("picker:search-requested"),
  onWindowSaved: on<{ revision: number; saved: boolean }>("picker:window-saved"),
  onWindowReset: on<{ revision: number; undo: number | null }>("picker:window-reset"),
  /** The shortcut changed, such as after a keyboard layout switch on macOS. */
  onShortcutStatus: on<ShortcutStatus>("shortcut:status"),
  onPreferencesChanged: on<PreferenceChange>("preferences:changed"),
  /** Saved-definition metadata can change without retargeting the current task. */
  onWritingsChanged: on("writings:changed"),
  onProjectChanged: on<[number, number] | null>("project:changed"),
  /** A chosen bibliography could not be saved, so the old choice stands. */
  onProjectNotSaved: on("project:not-saved"),
  /** An editor began or stopped waiting for a citation. */
  onEditorRequest: on("editor:request"),
  onEditorEndpoint: on<EndpointStatus>("editor:endpoint"),
  onLibraryChanged: on<LibraryStatus>("library:changed"),
  /** Presence only: native paths never enter the webview. */
  onFileDrag: on<boolean>("library:file-drag"),
  /** A native-accepted drop supersedes the explicit source chooser. */
  onLibraryFileDropped: on("library:file-dropped"),
  /** Dropped files Ibis cannot use: only that it happened, never a path. */
  onLibraryFileRefused: on("library:file-refused"),
  onUpdateProgress: on<UpdateProgress>("update:progress"),
  /** The saved automatic update choice, after a change from any menu. */
  onUpdateChecks: on<boolean>("update:automatic"),
  /** Settings is asked to show a page, such as the paste guide after a refused paste. */
  onSettingsPage: on<string>("settings:page"),
  onNoticeShow: on<NoticeMessage>("notice:show"),
  onNoticeLeave: on("notice:leave"),
  onAccentChanged: on<unknown>("accent:changed"),
  onAppearanceChanged: on<unknown>("appearance:changed"),
} as const;

type Options = {
  /** Once the handler is listening, so a read made now misses no change. */
  ready?: () => void;
  /** Native code refused the subscription. */
  failed?: () => void;
  /** Names the safe message logged if stopping fails, such as "Picker". */
  subject?: string;
};

/** Listens until the returned stop. Nothing is called after stop, and a
 * subscription that arrives later is stopped as soon as it exists. */
export function listening<H extends Handler>(source: Source<H> | undefined, handler: H, { ready, failed, subject = "Native" }: Options = {}): Stop {
  if (!source) return () => {};
  let active = true;
  const pending = source(((...args: Parameters<H>) => active ? handler(...args) : undefined) as H);
  pending.then(() => { if (active) ready?.(); }, () => { if (active) failed?.(); });
  return () => {
    active = false;
    void pending.then((stop) => stop(), () => undefined).catch(() => console.error(`${subject} event cleanup failed.`));
  };
}

/** `listening` for a component's lifetime, or until `key` changes. The latest
 * handler is called, so it may read current props without resubscribing. */
export function useNativeEvent<H extends Handler>(source: Source<H> | undefined, handler: H, { key, subject, ...callbacks }: Options & { key?: unknown } = {}): void {
  const handle = useEffectEvent((...args: Parameters<H>) => handler(...args));
  const ready = useEffectEvent(() => callbacks.ready?.());
  const failed = useEffectEvent(() => callbacks.failed?.());
  // biome-ignore lint/correctness/useExhaustiveDependencies: key restarts the subscription by design.
  useEffect(() => listening(source, handle as unknown as H, { ready, failed, subject }), [source, subject, key]);
}
