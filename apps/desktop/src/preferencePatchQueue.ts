import { commands, type KeyFormat, type PickerPreferences, type PickerSave } from "./bindings";

type Patch = Partial<PickerPreferences>;
type Intent = { order: number; revision: Promise<number> };
type Revisions = Partial<Record<keyof PickerPreferences, Intent>>;
type KeyChoice = { value: KeyFormat; saved: boolean; intent: Intent };

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
  private durable: PickerPreferences | null = null;
  private patch: Patch = {};
  private revisions: Revisions = {};
  private key: KeyChoice | null = null;
  private revision = 0;
  private reads = 0;
  private notifications = 0;
  private foreignNotifications = 0;
  private ownNotifications = new Map<number, { completed: boolean; notified: boolean }>();
  private pending = 0;
  private saving: Promise<unknown> = Promise.resolve();
  private loading: Promise<void> = Promise.resolve();

  constructor(
    private api: {
      read: () => Promise<PreferenceRead>;
      revision?: () => Promise<number>;
      write: (patch: Patch, key: KeyFormat | null, revision: number) => Promise<PickerSave>;
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

  /** Last acknowledged state, excluding replies superseded by other writers. */
  get savedPreferences(): PickerPreferences | null { return this.durable; }

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
    const notification = this.notifications;
    const fields = Object.keys(change) as (keyof PickerPreferences)[];
    // Dispatch issuance in the input handler, before waiting for any local
    // save or read. A reply arriving late cannot change this action's identity.
    if (fields.length || key !== null) {
      const issued = (this.api.revision ?? commands.preferenceRevision)();
      void issued.catch(() => undefined);
      const intent = { order: revision, revision: issued };
      if (key !== null) this.key = { value: key, saved: false, intent };
      for (const field of fields) this.revisions[field] = intent;
    }
    this.patch = { ...this.patch, ...change };
    // Never replace unreadable settings with defaults. Pre-load edits wait.
    if (!this.loaded) return;
    const queued = { ...this.patch };
    const revisions = { ...this.revisions };
    const deliberate = new Set(Object.keys(change));
    const chosenKey = this.key;
    const save = this.enqueue(async () => {
      const groups = new Map<Intent, { patch: Patch; key: KeyFormat | null }>();
      const group = (intent: Intent) => {
        if (!groups.has(intent)) groups.set(intent, { patch: {}, key: null });
        return groups.get(intent)!;
      };
      for (const field of Object.keys(queued) as (keyof PickerPreferences)[]) {
        const intent = revisions[field];
        if (intent && (deliberate.has(field) || this.revisions[field] === intent)) {
          Object.assign(group(intent).patch, { [field]: queued[field] });
        }
      }
      if (chosenKey && !chosenKey.saved) group(chosenKey.intent).key = chosenKey.value;
      let saved: PickerSave | undefined;
      for (const [intent, { patch, key }] of [...groups].sort(([a], [b]) => a.order - b.order)) {
        const issued = await intent.revision.catch((error: unknown) => {
          // An issuance failure has no ticket to retry; later choices remain usable.
          this.retire(intent, patch, chosenKey);
          throw error;
        });
        const own = { completed: false, notified: false };
        // Settings has versioned notifications and needs authoritative rollback.
        // The picker uses its existing reload path and retains no notice records.
        if (this.policy.restoreOnFailure) this.ownNotifications.set(issued, own);
        const foreign = this.foreignNotifications;
        let completed = false;
        try {
          saved = await this.api.write(patch, key, issued);
          completed = true;
          if (foreign === this.foreignNotifications) this.durable = saved.preferences;
        } finally {
          own.completed = true;
          if ((!completed || own.notified) && this.ownNotifications.get(issued) === own) this.ownNotifications.delete(issued);
        }
        this.retire(intent, patch, chosenKey);
      }
      if (saved && revision === this.revision && notification === this.notifications && (!this.policy.readAfterWrites || reading === this.reads)) {
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

  private retire(intent: Intent, patch: Patch, key: KeyChoice | null) {
    for (const field of Object.keys(patch) as (keyof PickerPreferences)[]) {
      if (this.revisions[field] === intent) { delete this.patch[field]; delete this.revisions[field]; }
    }
    if (key?.intent === intent) { key.saved = true; if (this.key === key) this.key = null; }
  }

  load(active: () => boolean = () => true): Promise<void> {
    return this.read(active, false);
  }

  /** Notifications invalidate old rendering replies; self-notifications retain
   * rollback acknowledgements. Refresh waits for actual completion, not closing. */
  async refresh(revision: number, active: () => boolean = () => true): Promise<void> {
    const own = this.ownNotifications.get(revision);
    if (own) {
      own.notified = true;
      if (own.completed) this.ownNotifications.delete(revision);
    } else this.foreignNotifications += 1;
    const notification = ++this.notifications;
    this.reads += 1;
    await this.drainWrites();
    if (active() && notification === this.notifications) await this.load(active);
  }

  private async drainWrites(): Promise<void> {
    while (this.pending) await this.saving.catch(() => undefined);
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
        this.durable = value.preferences;
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
      await this.drainWrites();
    };
    await Promise.race([
      drain().catch(() => undefined),
      new Promise<void>((resolve) => { timer = setTimeout(resolve, 2_000); }),
    ]);
    clearTimeout(timer);
  }
}
