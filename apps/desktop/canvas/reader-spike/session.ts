// One owner for the reader's document (#1252). Each open has an immutable
// identity; a completion that is no longer current destroys what it made and
// reports nothing, so a slow A can never replace a newer B.
import { pageNumbers, type PageNumber } from "./pageNumbers";

/** Limits from the reader decision (docs/decisions.md 0035). */
export const MAX_PDF_BYTES = 256 * 1024 * 1024;
export const OPEN_TIMEOUT_MS = 15_000;

export type EngineDocument = { numPages: number; getPageLabels(): Promise<string[] | null> };
/** Destroying the task frees its document, worker state and transferred bytes. */
export type EngineTask<D extends EngineDocument> = { promise: Promise<D>; destroy(): Promise<void> };
/** The PDF engine behind the adapter; PDF.js in the app, a fake in tests. */
export type Engine<D extends EngineDocument> = { open(bytes: Uint8Array): EngineTask<D> };

export type Failure = "too-large" | "encrypted" | "damaged" | "timed-out" | "unreadable";
export type Opened<D> = { kind: "opened"; load: Load; document: D; pages: PageNumber[] };
export type Outcome<D> = Opened<D> | { kind: "failed"; load: Load; failure: Failure } | { kind: "stale" };

/** Who asked and for what; never changes after creation. */
export type Load = Readonly<{ id: number; attachment: string }>;

export function failureOf(error: unknown): Failure {
  const name = error && typeof error === "object" && "name" in error ? String(error.name) : "";
  if (name === "PasswordException") return "encrypted";
  if (name === "InvalidPDFException" || name === "FormatError") return "damaged";
  return "unreadable";
}

export class ReaderSession<D extends EngineDocument> {
  #engine: Engine<D>;
  #timeout: number;
  #next = 0;
  #current: { load: Load; abort: AbortController; task: EngineTask<D> | null } | null = null;

  constructor(engine: Engine<D>, timeout = OPEN_TIMEOUT_MS) { this.#engine = engine; this.#timeout = timeout; }

  get current(): Load | null { return this.#current?.load ?? null; }

  /** `attachment` is an opaque engine id, never a path; `bytes` is the IPC read. */
  async open(attachment: string, bytes: (signal: AbortSignal) => Promise<Uint8Array>): Promise<Outcome<D>> {
    this.close();
    const load: Load = Object.freeze({ id: ++this.#next, attachment });
    const slot = { load, abort: new AbortController(), task: null as EngineTask<D> | null };
    this.#current = slot;
    const owned = () => this.#current === slot;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const timedOut = new Promise<"timed-out">((resolve) => { timer = setTimeout(() => resolve("timed-out"), this.#timeout); });
    try {
      const work = (async () => {
        const data = await bytes(slot.abort.signal);
        if (!owned()) return "stale" as const;
        if (data.byteLength > MAX_PDF_BYTES) return "too-large" as const;
        const task = this.#engine.open(data);
        slot.task = task;
        const document = await task.promise;
        if (!owned()) { await task.destroy(); return "stale" as const; }
        return { document, pages: pageNumbers(await document.getPageLabels(), document.numPages) };
      })();
      const result = await Promise.race([work, timedOut]);
      if (!owned()) return { kind: "stale" };
      if (result === "stale") return { kind: "stale" };
      if (result === "timed-out" || result === "too-large") { this.close(); return { kind: "failed", load, failure: result }; }
      return { kind: "opened", load, document: result.document, pages: result.pages };
    } catch (error) {
      if (!owned()) return { kind: "stale" };
      this.close();
      return { kind: "failed", load, failure: failureOf(error) };
    } finally {
      clearTimeout(timer);
    }
  }

  /** Hiding the picker, closing the panel or opening another source. */
  close(): void {
    const slot = this.#current;
    this.#current = null;
    if (!slot) return;
    slot.abort.abort();
    void slot.task?.destroy().catch(() => {});
  }
}
