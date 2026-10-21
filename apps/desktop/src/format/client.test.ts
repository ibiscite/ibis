import { CANCELLED, FormatClient, TIMED_OUT, type FormatReply, type FormatRequest, type WorkerLike } from "./client";

class FakeWorker implements WorkerLike {
  onmessage: ((event: MessageEvent<FormatReply>) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;
  requests: FormatRequest[] = [];
  terminated = false;
  postMessage(request: FormatRequest) {
    this.requests.push(request);
  }
  terminate() {
    this.terminated = true;
  }
  reply(reply: FormatReply) {
    this.onmessage?.({ data: reply } as MessageEvent<FormatReply>);
  }
}

/** Answers one request at a time, each after `eachMs`, like the real worker. */
class SerialWorker extends FakeWorker {
  private queue: FormatRequest[] = [];
  constructor(private eachMs: number) {
    super();
  }
  override postMessage(request: FormatRequest) {
    super.postMessage(request);
    this.queue.push(request);
    if (this.queue.length === 1) this.work();
  }
  private work() {
    const next = this.queue[0];
    if (!next) return;
    setTimeout(() => {
      if (this.terminated) return;
      this.queue.shift();
      this.reply({ id: next.id, result: { output: `#${next.id}`, problems: [] } });
      this.work();
    }, this.eachMs);
  }
}

const request = { kind: "bibliography" as const, style: "apa" as const, format: "text" as const, items: [] };

describe("formatting client", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("returns the worker's answer", async () => {
    const worker = new FakeWorker();
    const client = new FormatClient(() => worker, 1000);
    const result = client.format(request);
    const sent = worker.requests[0];
    worker.reply({ id: sent?.id ?? 0, result: { output: "Garcia, A. (2021).", problems: [] } });
    await expect(result).resolves.toEqual({ output: "Garcia, A. (2021).", problems: [] });
  });

  it("stops a worker that takes too long and serves the next request with a fresh one", async () => {
    const workers: FakeWorker[] = [];
    const client = new FormatClient(() => {
      const worker = new FakeWorker();
      workers.push(worker);
      return worker;
    }, 1000);
    const slow = client.format(request);
    const waiting = client.format(request);
    // Requests go to the worker one at a time.
    expect(workers[0]?.requests).toHaveLength(1);
    vi.advanceTimersByTime(1000);
    await expect(slow).rejects.toThrow(TIMED_OUT);
    expect(workers[0]?.terminated).toBe(true);

    // The waiting request never reached the stopped worker, so it runs on a new one.
    expect(workers).toHaveLength(2);
    // A late answer from the stopped worker is ignored.
    workers[0]?.reply({ id: 1, result: { output: "late", problems: [] } });
    workers[1]?.reply({ id: workers[1].requests[0]?.id ?? 0, result: { output: "fresh", problems: [] } });
    await expect(waiting).resolves.toEqual({ output: "fresh", problems: [] });
  });

  it("keeps a busy but healthy worker whose queue is longer than the limit", async () => {
    const worker = new SerialWorker(400);
    const client = new FormatClient(() => worker, 1000);
    const replies = [1, 2, 3, 4].map(() => client.format(request).then((r) => r.output, (e: Error) => e.message));
    await vi.advanceTimersByTimeAsync(2000);
    expect(worker.terminated).toBe(false);
    expect(await Promise.all(replies)).toEqual(["#1", "#2", "#3", "#4"]);
  });

  it("times a request from when the worker starts it, and still stops a stuck one", async () => {
    const workers: FakeWorker[] = [];
    const client = new FormatClient(() => {
      const worker = new FakeWorker();
      workers.push(worker);
      return worker;
    }, 1000);
    const first = client.format(request);
    const stuck = client.format(request);
    const after = client.format(request);
    vi.advanceTimersByTime(900);
    workers[0]?.reply({ id: workers[0].requests[0]?.id ?? 0, result: { output: "first", problems: [] } });
    await expect(first).resolves.toEqual({ output: "first", problems: [] });
    // The stuck request started at 900 ms, so it has until 1900 ms.
    expect(workers[0]?.requests).toHaveLength(2);
    vi.advanceTimersByTime(999);
    expect(workers[0]?.terminated).toBe(false);
    vi.advanceTimersByTime(1);
    expect(workers[0]?.terminated).toBe(true);
    await expect(stuck).rejects.toThrow(TIMED_OUT);
    workers[1]?.reply({ id: workers[1].requests[0]?.id ?? 0, result: { output: "after", problems: [] } });
    await expect(after).resolves.toEqual({ output: "after", problems: [] });
  });

  it("drops cancelled requests before they reach the worker", async () => {
    const worker = new FakeWorker();
    const client = new FormatClient(() => worker, 1000);
    const running = client.format(request);
    const replaced = Array.from({ length: 50 }, () => {
      const cancel = new AbortController();
      const result = client.format(request, cancel.signal);
      cancel.abort();
      return result;
    });
    const newest = client.format(request);
    const already = new AbortController();
    already.abort();
    await expect(client.format(request, already.signal)).rejects.toThrow(CANCELLED);
    for (const result of replaced) await expect(result).rejects.toThrow(CANCELLED);

    worker.reply({ id: worker.requests[0]?.id ?? 0, result: { output: "running", problems: [] } });
    await expect(running).resolves.toEqual({ output: "running", problems: [] });
    // Only the newest request is sent next; the 50 replaced ones never were.
    expect(worker.requests).toHaveLength(2);
    worker.reply({ id: worker.requests[1]?.id ?? 0, result: { output: "newest", problems: [] } });
    await expect(newest).resolves.toEqual({ output: "newest", problems: [] });
  });

  it("lets a request the worker already started finish when it is cancelled", async () => {
    const worker = new FakeWorker();
    const client = new FormatClient(() => worker, 1000);
    const cancel = new AbortController();
    const started = client.format(request, cancel.signal);
    cancel.abort();
    const next = client.format(request);
    worker.reply({ id: worker.requests[0]?.id ?? 0, result: { output: "started", problems: [] } });
    await expect(started).resolves.toEqual({ output: "started", problems: [] });
    worker.reply({ id: worker.requests[1]?.id ?? 0, result: { output: "next", problems: [] } });
    await expect(next).resolves.toEqual({ output: "next", problems: [] });
  });

  it("reports a crashed worker instead of waiting forever", async () => {
    const worker = new FakeWorker();
    const client = new FormatClient(() => worker, 1000);
    const result = client.format(request);
    worker.onerror?.(new Event("error"));
    await expect(result).rejects.toThrow("the citation engine stopped unexpectedly");
  });

  it("fails only the request that could not be sent", async () => {
    const worker = new FakeWorker();
    const client = new FormatClient(() => worker, 1000);
    const post = worker.postMessage.bind(worker);
    worker.postMessage = () => {
      throw new Error("DataCloneError");
    };
    await expect(client.format(request)).rejects.toThrow("the references could not be sent to the formatter");
    worker.postMessage = post;
    const next = client.format(request);
    vi.advanceTimersByTime(999);
    worker.reply({ id: worker.requests[0]?.id ?? 0, result: { output: "ok", problems: [] } });
    await expect(next).resolves.toEqual({ output: "ok", problems: [] });
    expect(worker.terminated).toBe(false);
  });

  it("reports a worker that cannot start", async () => {
    const client = new FormatClient(() => {
      throw new Error("no workers here");
    }, 1000);
    await expect(client.format(request)).rejects.toThrow("the citation engine stopped unexpectedly");
  });
});
