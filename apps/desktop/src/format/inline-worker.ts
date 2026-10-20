// Test helper: runs the real engine on the calling thread in place of a worker.
import type { FormatReply, FormatRequest, WorkerLike } from "./client";
import { Formatter } from "./engine";

/** Runs the real engine in the test instead of a worker. */
export class InlineWorker implements WorkerLike {
  onmessage: ((event: MessageEvent<FormatReply>) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;
  private formatter = new Formatter();
  postMessage(request: FormatRequest) {
    const result = request.kind === "citation"
      ? this.formatter.citation(request.style, request.items, request.cites, request.format)
      : this.formatter.bibliography(request.style, request.items, request.format);
    queueMicrotask(() => this.onmessage?.({ data: { id: request.id, result } } as MessageEvent<FormatReply>));
  }
  terminate() {}
}
