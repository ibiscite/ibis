// Sends formatting requests to the worker and stops it when one runs too
// long, so a pathological style or item can never freeze the picker.
import type { CiteItem, CslItem, Formatted, OutputFormat, StyleId } from "./engine";

type Request =
  | { kind: "citation"; style: StyleId; format: OutputFormat; items: CslItem[]; cites: CiteItem[] }
  | { kind: "bibliography"; style: StyleId; format: OutputFormat; items: CslItem[] };

export type FormatRequest = Request & { id: number };
export type FormatReply = { id: number; result: Formatted } | { id: number; error: string };

/** The part of `Worker` the client uses, so tests can supply their own. */
export interface WorkerLike {
  onmessage: ((event: MessageEvent<FormatReply>) => void) | null;
  onerror: ((event: Event) => void) | null;
  postMessage(request: FormatRequest): void;
  terminate(): void;
}

export const TIMED_OUT = "formatting took too long and was stopped";
const STOPPED = "the citation engine stopped unexpectedly";

function startWorker(): WorkerLike {
  return new Worker(new URL("./worker.ts", import.meta.url), { type: "module" }) as unknown as WorkerLike;
}

interface Pending {
  resolve(result: Formatted): void;
  reject(error: Error): void;
  timer: ReturnType<typeof setTimeout>;
}

export class FormatClient {
  private worker: WorkerLike | null = null;
  private pending = new Map<number, Pending>();
  private next = 0;

  constructor(private start: () => WorkerLike = startWorker, private limitMs = 5000) {}

  format(request: Request): Promise<Formatted> {
    return new Promise((resolve, reject) => {
      const id = ++this.next;
      let worker: WorkerLike;
      try {
        worker = this.current();
      } catch {
        reject(new Error(STOPPED));
        return;
      }
      const timer = setTimeout(() => this.stop(TIMED_OUT), this.limitMs);
      this.pending.set(id, { resolve, reject, timer });
      try {
        worker.postMessage({ ...request, id });
      } catch {
        // Only this request failed; the worker and other requests carry on.
        clearTimeout(timer);
        this.pending.delete(id);
        reject(new Error("the references could not be sent to the formatter"));
      }
    });
  }

  private current(): WorkerLike {
    if (!this.worker) {
      const worker = this.start();
      worker.onmessage = (event) => this.settle(event.data);
      worker.onerror = () => this.stop(STOPPED);
      this.worker = worker;
    }
    return this.worker;
  }

  private settle(reply: FormatReply) {
    const pending = this.pending.get(reply.id);
    if (!pending) return;
    this.pending.delete(reply.id);
    clearTimeout(pending.timer);
    if ("error" in reply) pending.reject(new Error(reply.error));
    else pending.resolve(reply.result);
  }

  /** Stop the worker and fail everything it was doing; the next request starts a new one. */
  private stop(reason: string) {
    if (this.worker) {
      this.worker.onmessage = null;
      this.worker.onerror = null;
      this.worker.terminate();
    }
    this.worker = null;
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timer);
      pending.reject(new Error(reason));
    }
    this.pending.clear();
  }
}
