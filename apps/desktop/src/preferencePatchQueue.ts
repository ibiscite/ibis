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
 * submitted when the person makes it, carrying only its own fields and any
 * whose save failed, never a field still being saved. */
export class PreferencePatchQueue {
  private listeners = new Set<(event: Event) => void>();
  private loaded = false;
  private durable: PickerPreferences | null = null;
  /** Choices not yet acknowledged, shown over the durable state. */
  private patch: Patch = {};
  /** Fields whose save failed, sent again with the next choice. */
  private retry: Patch = {};
  /** Choices made before the first successful read, sent once it arrives. */
  private held: Patch = {};
  /** Which choice last set each field, so an equal value chosen again keeps its own fate. */
  private latest: Partial<Record<keyof PickerPreferences, number>> = {};
  /** Choices the person made; resending a failure is not one. */
  private chosen = 0;
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
      /** Hands a choice to the settings owner at once; resolves with its receipt. */
      submit: (patch: Patch) => Promise<number>;
      /** How a submitted choice went. */
      saved: (receipt: number) => Promise<PickerSave>;
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

  change(change: Patch): void {
    const revision = ++this.revision;
    if (Object.keys(change).length) this.chosen += 1;
    const chosen = this.chosen;
    const reading = this.reads;
    const notification = this.notifications;
    this.patch = { ...this.patch, ...change };
    // Never replace unreadable settings with defaults. Pre-load edits wait.
    if (!this.loaded) {
      this.held = { ...this.held, ...change };
      return;
    }
    const sent: Patch = { ...this.retry, ...this.held, ...change };
    this.retry = {};
    this.held = {};
    const fields = Object.keys(sent) as (keyof PickerPreferences)[];
    for (const field of fields) this.latest[field] = revision;
    if (!Object.keys(sent).length) return;
    // Submitted now, in the input handler, so the owner orders it by when it was chosen.
    const receipt = this.api.submit(sent);
    // A choice saved in the other window after this one retires its failure.
    const foreign = this.foreignNotifications;
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
        saved = await this.api.saved(issued);
        if (foreign === this.foreignNotifications) this.durable = saved.preferences;
        this.acknowledge(fields, revision);
      } catch (error) {
        for (const field of fields) {
          if (this.latest[field] !== revision) continue;
          // The other window chose since: its choice stands and this one is dropped.
          if (foreign !== this.foreignNotifications) { delete this.patch[field]; delete this.latest[field]; }
          // Otherwise kept to send again.
          else Object.assign(this.retry, { [field]: sent[field] });
        }
        // The person chose again before this failure was known, so that choice
        // could not carry the retry: send it once now, behind it. A resend is
        // not a choice, so failures never resend each other.
        if (chosen !== this.chosen && Object.keys(this.retry).length) queueMicrotask(() => this.change({}));
        throw error;
      } finally {
        own.completed = true;
        if (issued !== undefined && (!saved || own.notified) && this.ownNotifications.get(issued) === own) this.ownNotifications.delete(issued);
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

  /** A saved field leaves the overlay, unless chosen again since. */
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
  async refresh(revision: number, active: () => boolean = () => true): Promise<void> {
    const own = this.ownNotifications.get(revision);
    if (own) {
      own.notified = true;
      if (own.completed) this.ownNotifications.delete(revision);
    } else {
      this.foreignNotifications += 1;
      this.durable = null;
      // A choice made in the other window is newer than any retry here.
      for (const field of Object.keys(this.retry) as (keyof PickerPreferences)[]) { delete this.patch[field]; delete this.latest[field]; }
      this.retry = {};
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
        this.retry = {};
        this.held = {};
        this.latest = {};
      }
      const pendingOutput = this.patch.output !== undefined;
      if (value.preferences) {
        this.loaded = true;
        this.durable = value.preferences;
        // Picker reads preserve optimistic choices. Settings focus reads show
        // durable state, even if a failed recovery left retry fields pending.
        if (this.policy.readAfterWrites) value = { ...value, preferences: { ...value.preferences, ...this.patch } };
      }
      this.emit({ type: "read", value, pendingOutput });
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
