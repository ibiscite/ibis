import type { PickerPreferences, PickerSave } from "./bindings";

type Patch = Partial<PickerPreferences>;

/** Null when the preferences cannot be read: nothing is replaced with defaults. */
export type PreferenceRead = { preferences: PickerPreferences | null };
type Event =
  | { type: "read"; value: PreferenceRead; pendingOutput: boolean }
  | { type: "saved"; value: PickerSave }
  | { type: "invalidated" }
  | { type: "saving"; promise: Promise<void> }
  | { type: "error"; source: "read" | "save" };

/** One queue per window. The settings owner keeps durable state and applies
 * choices in the order they were made (decision 0040): each choice is
 * submitted when the person makes it, with only its own fields. A choice that
 * fails to save is never sent again later, when it could land over a newer
 * one. Settings shows what is saved; the picker keeps applying it until Ibis
 * restarts, or until the other window saves that field after it. */
export class PreferencePatchQueue {
  private listeners = new Set<(event: Event) => void>();
  private loaded = false;
  private durable: PickerPreferences | null = null;
  /** Choices not yet acknowledged, shown over the durable state. */
  private patch: Patch = {};
  /** Which choice last set each field, so an equal value chosen again keeps its own fate. */
  private latest: Partial<Record<keyof PickerPreferences, number>> = {};
  /** The picker's choices that failed to save, by the owner's receipt, and the
   * newest receipt the other window saved each field with: the later stands. */
  private failed: Partial<Record<keyof PickerPreferences, number>> = {};
  private foreign: Partial<Record<keyof PickerPreferences, number>> = {};
  private revision = 0;
  /** Saves acknowledged so far: a read that started before one is older than it. */
  private acknowledged = 0;
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
      /** Hands a choice to the settings owner at once; resolves with its receipt. */
      submit: (patch: Patch) => Promise<number>;
      /** How a submitted choice went. */
      saved: (receipt: number) => Promise<PickerSave>;
    },
    private policy: {
      /** Picker notifications read after its writes; Settings focus reads skip busy state. */
      readAfterWrites?: boolean;
      /** Settings records its own notifications, for rollback after a failed save. */
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

  change(change: Patch): void {
    const fields = Object.keys(change) as (keyof PickerPreferences)[];
    if (!fields.length) return;
    const revision = ++this.revision;
    const reading = this.reads;
    const notification = this.notifications;
    this.patch = { ...this.patch, ...change };
    for (const field of fields) { this.latest[field] = revision; delete this.failed[field]; }
    // Submitted now, in the input handler, so the owner orders it by when it was
    // chosen. A patch names only its own fields, so unreadable settings are
    // never replaced with defaults: the owner refuses it.
    const receipt = this.api.submit(change);
    void receipt.catch(() => undefined);
    const save = this.enqueue(async () => {
      let saved: PickerSave | undefined;
      let issued: number | undefined;
      const own = { completed: false, notified: false };
      try {
        issued = await receipt;
        // Settings has versioned notifications and needs authoritative rollback.
        // The picker uses its existing reload path and retains no notice records.
        if (this.policy.restoreOnFailure) this.ownNotifications.set(issued, own);
        const foreign = this.foreignNotifications;
        saved = await this.api.saved(issued);
        if (foreign === this.foreignNotifications) this.durable = saved.preferences;
        this.acknowledged += 1;
        this.acknowledge(fields, revision);
      } catch (error) {
        // Not saved. Settings shows what is saved; the picker keeps the choice
        // applied, unsaved, and never sends it again.
        if (this.policy.restoreOnFailure) this.acknowledge(fields, revision);
        else {
          for (const field of fields) {
            if (this.latest[field] !== revision) continue;
            // The other window already saved this field after it: its value shows.
            if ((this.foreign[field] ?? -Infinity) > (issued ?? 0)) { delete this.patch[field]; delete this.latest[field]; }
            else this.failed[field] = issued ?? 0;
          }
        }
        throw error;
      } finally {
        own.completed = true;
        if (issued !== undefined && (!saved || own.notified) && this.ownNotifications.get(issued) === own) this.ownNotifications.delete(issued);
      }
      if (saved && revision === this.revision && notification === this.notifications && (!this.policy.readAfterWrites || reading === this.reads)) {
        // The picker keeps showing choices that apply but are not saved, as its reads do.
        this.emit({ type: "saved", value: this.policy.readAfterWrites ? { preferences: { ...saved.preferences, ...this.patch } } : saved });
      }
    }).catch((error: unknown) => {
      this.emit({ type: "error", source: "save" });
      if (this.policy.restoreOnFailure && this.pending === 0 && revision === this.revision) void this.read(() => true, true);
      throw error;
    });
    this.saving = save;
    // Subscribers may show progress/failure; the queue always handles rejection.
    void save.catch(() => undefined);
    this.emit({ type: "saving", promise: save });
  }

  /** A settled field leaves the overlay, unless chosen again since. */
  private acknowledge(fields: (keyof PickerPreferences)[], choice: number) {
    for (const field of fields) {
      if (this.latest[field] === choice) { delete this.patch[field]; delete this.latest[field]; }
    }
  }

  load(active: () => boolean = () => true): Promise<void> {
    return this.read(active, false);
  }

  /** Notifications invalidate old rendering replies; self-notifications retain
   * rollback acknowledgements. Refresh waits for actual completion, not closing. */
  async refresh(revision: number, active: () => boolean = () => true, fields?: readonly string[]): Promise<void> {
    const own = this.ownNotifications.get(revision);
    if (own) {
      own.notified = true;
      if (own.completed) this.ownNotifications.delete(revision);
    } else {
      this.foreignNotifications += 1;
      this.durable = null;
      // The other window saved these fields after a choice here that failed: its value shows.
      for (const field of (fields ?? Object.keys(this.failed)) as (keyof PickerPreferences)[]) {
        this.foreign[field] = Math.max(this.foreign[field] ?? -Infinity, revision);
        const failed = this.failed[field];
        if (failed !== undefined && failed < revision) { delete this.failed[field]; delete this.patch[field]; delete this.latest[field]; }
      }
      this.emit({ type: "invalidated" });
    }
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
    const acknowledged = this.acknowledged;
    const revision = this.revision;
    const loadedBefore = this.loaded;
    const idle = this.pending === 0;
    const current = () => active() && request === this.reads
      && (!loadedBefore || revision === this.revision)
      && (this.policy.readAfterWrites || idle && this.pending === 0);
    const read = this.policy.readAfterWrites
      ? this.saving.catch(() => undefined).then(() => this.api.read())
      : this.api.read();
    this.loading = read.then((value): Promise<void> | void => {
      if (!current()) return;
      // A save landed while this read was out, so the read is older: read again.
      if (acknowledged !== this.acknowledged) return this.read(active, restore);
      if (restore) {
        this.patch = {};
        this.latest = {};
        this.failed = {};
      }
      const pendingOutput = this.patch.output !== undefined;
      if (value.preferences) {
        this.loaded = true;
        this.durable = value.preferences;
        // Picker reads keep showing choices still being saved. Settings focus
        // reads show durable state.
        if (this.policy.readAfterWrites) value = { ...value, preferences: { ...value.preferences, ...this.patch } };
      }
      this.emit({ type: "read", value, pendingOutput });
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
