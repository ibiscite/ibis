import type { Source, Stop } from "../nativeEvents";
import type { SettingsApi } from "./api";

function stopListening(stop: Stop) {
  try { stop(); } catch { console.error("Settings event cleanup failed."); }
}

/** The lifetime of a reply and its subscription. A retired request has no
 * continuation. Work that must finish uses the save queue before entering here. */
export class RequestScope {
  private controller = new AbortController();
  readonly signal = this.controller.signal;

  end() { this.controller.abort(); }

  run<T>(request: () => Promise<T>, discard?: (value: T) => void): Promise<T> {
    return new Promise((resolve, reject) => {
      if (this.signal.aborted) return;
      void request().then((value) => {
        if (this.signal.aborted) discard?.(value);
        else resolve(value);
      }, (error: unknown) => { if (!this.signal.aborted) reject(error); });
    });
  }

  async subscribe<H extends (...args: never[]) => unknown>(source: Source<H>, handler: H): Promise<Stop> {
    const guarded = ((...args: Parameters<H>) => {
      if (!this.signal.aborted) return handler(...args);
    }) as H;
    const unsubscribe = await this.run(() => source(guarded), stopListening);
    let listening = true;
    const stop = () => {
      if (!listening) return;
      listening = false;
      this.signal.removeEventListener("abort", onEnd);
      unsubscribe();
    };
    const onEnd = () => stopListening(stop);
    if (this.signal.aborted) onEnd();
    else this.signal.addEventListener("abort", onEnd, { once: true });
    return stop;
  }
}

type Lifecycle = "onPresentation" | "onCloseRequested";
type NativeOwned = "chooseSettingsBackup" | "restoreSettingsBackup" | "checkForUpdates";
type ViewApi = Omit<SettingsApi, Lifecycle | NativeOwned> & {
  chooseSettingsBackup: () => ReturnType<SettingsApi["chooseSettingsBackup"]>;
  restoreSettingsBackup: (ticket: number) => ReturnType<SettingsApi["restoreSettingsBackup"]>;
  checkForUpdates: () => ReturnType<SettingsApi["checkForUpdates"]>;
};
const subscriptions = {
  onEditorEndpoint: true,
  onLibraryChanged: true,
  onPreferencesChanged: true,
  onSettingsChanged: true,
  onShortcutStatus: true,
  onUpdateChecks: true,
  onWindowSaved: true,
} satisfies Record<Extract<keyof ViewApi, `on${string}`>, true>;

/** The only API its page tree sees. The restore ticket releases a native
 * preview even if its reply arrives after the opening ends. */
export class Opening extends RequestScope {
  readonly api: ViewApi;

  constructor(readonly id: number, api: SettingsApi) {
    super();
    const { onPresentation: _present, onCloseRequested: _close, cancelSettingsRestore, chooseSettingsBackup, restoreSettingsBackup, checkForUpdates, ...view } = api;
    const methods: [string, ((...args: never[]) => Promise<unknown>) | undefined][] = Object.entries(view);
    const scoped = Object.fromEntries(methods.map(([name, method]) => {
      if (!method) return [name, method];
      if (Object.hasOwn(subscriptions, name)) return [name, (handler: (...args: never[]) => unknown) => this.subscribe(method as Source<typeof handler>, handler)];
      return [name, (...args: never[]) => this.run(() => method(...args))];
    })) as typeof view;
    this.api = {
      ...scoped,
      cancelSettingsRestore,
      chooseSettingsBackup: () => this.run(() => chooseSettingsBackup(id), (preview) => {
        if (preview) void cancelSettingsRestore(preview.ticket).catch(() => undefined);
      }),
      restoreSettingsBackup: (ticket) => this.run(() => restoreSettingsBackup(ticket, id)),
      checkForUpdates: () => this.run(() => checkForUpdates(id)),
    };
  }
}
