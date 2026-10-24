// Sends formatting requests to the worker and stops it when one runs too
// long, so a pathological style or item can never freeze the picker.
import type { CiteItem, CslItem, Formatted, OutputFormat, StyleId } from "./engine";

type Request =
  | { kind: "citation"; style: StyleId; format: OutputFormat; items: CslItem[]; cites: CiteItem[]; segments?: boolean }
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
export const CANCELLED = "formatting was cancelled before it started";
const STOPPED = "the citation engine stopped unexpectedly";

function startWorker(): WorkerLike {
  return new Worker(new URL("./worker.ts", import.meta.url), { type: "module" }) as unknown as WorkerLike;
}

interface Job {
  request: FormatRequest;
  resolve(result: Formatted): void;
  reject(error: Error): void;
  /** Stops listening for cancellation once the job leaves the queue. */
  release(): void;
}

/**
 * The worker runs one request at a time, so the client sends it one at a
 * time and keeps the rest in a queue. The time limit starts when the worker
 * starts a request, never while it waits, so a busy but healthy worker is not
 * stopped. A request cancelled while it waits is dropped without being sent.
 */
export class FormatClient {
  private worker: WorkerLike | null = null;
  private queue: Job[] = [];
  private running: Job | null = null;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private next = 0;

  constructor(private start: () => WorkerLike = startWorker, private limitMs = 5000) {}

  /**
   * `signal` cancels the request while it waits for the worker. Once the
   * worker has started it, it runs to the end and its result is returned.
   */
  format(request: Request, signal?: AbortSignal): Promise<Formatted> {
    return new Promise((resolve, reject) => {
      if (signal?.aborted) {
        reject(new Error(CANCELLED));
        return;
      }
      const job: Job = { request: { ...request, id: ++this.next }, resolve, reject, release: () => {} };
      if (signal) {
        const cancel = () => {
          const index = this.queue.indexOf(job);
          if (index < 0) return;
          this.queue.splice(index, 1);
          reject(new Error(CANCELLED));
        };
        signal.addEventListener("abort", cancel, { once: true });
        job.release = () => signal.removeEventListener("abort", cancel);
      }
      this.queue.push(job);
      this.pump();
    });
  }

  /** Send the next waiting request when the worker is free. */
  private pump() {
    while (!this.running) {
      const job = this.queue.shift();
      if (!job) return;
      job.release();
      let worker: WorkerLike;
      try {
        worker = this.current();
      } catch {
        job.reject(new Error(STOPPED));
        continue;
      }
      // Set before sending, in case a worker answers during postMessage.
      this.running = job;
      this.timer = setTimeout(() => this.stop(TIMED_OUT), this.limitMs);
      try {
        worker.postMessage(job.request);
      } catch {
        // Only this request failed; the worker and other requests carry on.
        if (this.running === job) this.finish();
        job.reject(new Error("the references could not be sent to the formatter"));
      }
    }
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

  /** Clear the running request and its timer, and return it. */
  private finish(): Job | null {
    const job = this.running;
    clearTimeout(this.timer);
    this.timer = undefined;
    this.running = null;
    return job;
  }

  private settle(reply: FormatReply) {
    if (reply.id !== this.running?.request.id) return;
    const job = this.finish()!;
    if ("error" in reply) job.reject(new Error(reply.error));
    else job.resolve(reply.result);
    this.pump();
  }

  /**
   * Stop the worker and fail the request it was running. Waiting requests
   * were never sent to it, so they go to a fresh worker.
   */
  private stop(reason: string) {
    if (this.worker) {
      this.worker.onmessage = null;
      this.worker.onerror = null;
      this.worker.terminate();
    }
    this.worker = null;
    this.finish()?.reject(new Error(reason));
    this.pump();
  }
}
