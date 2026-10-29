import type { KeyFormat, PickerPreferences, PickerSave } from "./bindings";

type Patch = Partial<PickerPreferences>;
type Revisions = Partial<Record<keyof PickerPreferences, number>>;

/** The picker can still read its key format when preferences are unreadable. */
export type PreferenceRead = {
  preferences: PickerPreferences | null;
  copy_format?: KeyFormat;
  formatFailed?: boolean;
};
type Event =
  | { type: "read"; value: PreferenceRead; pendingStyle: boolean }
  | { type: "saved"; value: PickerSave }
  | { type: "saving"; promise: Promise<void> }
  | { type: "error"; source: "read" | "save" };

/** One queue per window. The native lock remains the owner of durable state;
 * this queue sends only deliberate edits and unacknowledged retry fields. */
export class PreferencePatchQueue {
  private listeners = new Set<(event: Event) => void>();
  private loaded = false;
  private patch: Patch = {};
  private revisions: Revisions = {};
  private key: { value: KeyFormat; saved: boolean } | null = null;
  private revision = 0;
  private reads = 0;
  private pending = 0;
  private saving: Promise<unknown> = Promise.resolve();
  private loading: Promise<void> = Promise.resolve();

  constructor(
    private api: {
      read: () => Promise<PreferenceRead>;
      write: (patch: Patch, key: KeyFormat | null) => Promise<PickerSave>;
    },
    private policy: {
      /** Picker notifications read after its writes; Settings focus reads skip busy state. */
      readAfterWrites?: boolean;
      /** Settings restores durable choices; the picker retains failed choices for retry. */
      restoreOnFailure?: boolean;
    } = {},
  ) {}

  subscribe(listener: (event: Event) => void): () => void {
    this.listeners.add(listener);
    return () => { this.listeners.delete(listener); };
  }

  private emit(event: Event) {
    for (const listener of this.listeners) listener(event);
  }

  /** Also serializes Settings' shortcut writes with its preference writes. */
  enqueue<T>(action: () => Promise<T>): Promise<T> {
    this.pending += 1;
    const operation = this.saving.catch(() => undefined).then(action)
      .finally(() => { this.pending -= 1; });
    this.saving = operation;
    return operation;
  }

  change(change: Patch, key: KeyFormat | null = null): void {
    const revision = ++this.revision;
    const reading = this.reads;
    if (key !== null) this.key = { value: key, saved: false };
    this.patch = { ...this.patch, ...change };
    for (const field of Object.keys(change) as (keyof PickerPreferences)[]) this.revisions[field] = revision;
    // Never replace unreadable settings with defaults. Pre-load edits wait.
    if (!this.loaded) return;
    const queued = { ...this.patch };
    const revisions = { ...this.revisions };
    const deliberate = new Set(Object.keys(change));
    const chosenKey = this.key;
    const save = this.enqueue(async () => {
      const patch = Object.fromEntries(Object.entries(queued).filter(([field]) =>
        deliberate.has(field) || this.revisions[field as keyof PickerPreferences] === revisions[field as keyof PickerPreferences]));
      const saved = await this.api.write(patch, chosenKey && !chosenKey.saved ? chosenKey.value : null);
      for (const field of Object.keys(patch) as (keyof PickerPreferences)[]) {
        if (this.revisions[field] === revisions[field]) {
          delete this.patch[field];
          delete this.revisions[field];
        }
      }
      if (chosenKey) chosenKey.saved = true;
      if (this.key === chosenKey) this.key = null;
      if (revision === this.revision && (!this.policy.readAfterWrites || reading === this.reads)) {
        this.emit({ type: "saved", value: saved });
      }
    }).catch((error: unknown) => {
      this.emit({ type: "error", source: "save" });
      if (this.policy.restoreOnFailure && this.pending === 0 && revision === this.revision) {
        void this.read(() => true, true);
      }
      throw error;
    });
    this.saving = save;
    // Subscribers may show progress/failure; the queue always handles rejection.
    void save.catch(() => undefined);
    this.emit({ type: "saving", promise: save });
  }

  load(active: () => boolean = () => true): Promise<void> {
    return this.read(active, false);
  }

  private read(active: () => boolean, restore: boolean): Promise<void> {
    const request = ++this.reads;
    const revision = this.revision;
    const loadedBefore = this.loaded;
    const idle = this.pending === 0;
    const current = () => active() && request === this.reads
      && (!loadedBefore || revision === this.revision)
      && (this.policy.readAfterWrites || idle && this.pending === 0);
    const read = this.policy.readAfterWrites
      ? this.saving.catch(() => undefined).then(() => this.api.read())
      : this.api.read();
    this.loading = read.then((value) => {
      if (!current()) return;
      if (restore) {
        this.patch = {};
        this.revisions = {};
        this.key = null;
      }
      const pendingStyle = this.patch.citation_style !== undefined;
      if (value.preferences) {
        this.loaded = true;
        // Picker reads preserve optimistic choices. Settings focus reads show
        // durable state, even if a failed recovery left retry fields pending.
        if (this.policy.readAfterWrites) value = { ...value, preferences: { ...value.preferences, ...this.patch } };
      }
      this.emit({ type: "read", value, pendingStyle });
      if (value.preferences && this.policy.readAfterWrites && Object.keys(this.patch).length) this.change({});
    }).catch(() => {
      if (current()) this.emit({ type: "error", source: "read" });
    });
    return this.loading;
  }

  /** Drain choices, including ones queued while closing, without trapping a
   * person behind a stuck native command. Picker also waits for its first read. */
  async finishSaving(): Promise<void> {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const drain = async () => {
      if (this.policy.readAfterWrites) await this.loading;
      while (this.pending) await this.saving.catch(() => undefined);
    };
    await Promise.race([
      drain().catch(() => undefined),
      new Promise<void>((resolve) => { timer = setTimeout(resolve, 2_000); }),
    ]);
    clearTimeout(timer);
  }
}
