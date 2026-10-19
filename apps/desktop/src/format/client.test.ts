import { FormatClient, TIMED_OUT, type FormatReply, type FormatRequest, type WorkerLike } from "./client";

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

  it("stops a worker that takes too long and starts a fresh one", async () => {
    const workers: FakeWorker[] = [];
    const client = new FormatClient(() => {
      const worker = new FakeWorker();
      workers.push(worker);
      return worker;
    }, 1000);
    const slow = client.format(request);
    const alsoWaiting = client.format(request);
    vi.advanceTimersByTime(1000);
    await expect(slow).rejects.toThrow(TIMED_OUT);
    await expect(alsoWaiting).rejects.toThrow(TIMED_OUT);
    expect(workers[0]?.terminated).toBe(true);

    const next = client.format(request);
    expect(workers).toHaveLength(2);
    // A late answer from the stopped worker is ignored.
    workers[0]?.reply({ id: 1, result: { output: "late", problems: [] } });
    workers[1]?.reply({ id: workers[1].requests[0]?.id ?? 0, result: { output: "fresh", problems: [] } });
    await expect(next).resolves.toEqual({ output: "fresh", problems: [] });
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
    const waiting = client.format(request);
    const post = worker.postMessage.bind(worker);
    worker.postMessage = () => {
      throw new Error("DataCloneError");
    };
    await expect(client.format(request)).rejects.toThrow("the references could not be sent to the formatter");
    worker.postMessage = post;
    vi.advanceTimersByTime(999);
    worker.reply({ id: worker.requests[0]?.id ?? 0, result: { output: "ok", problems: [] } });
    await expect(waiting).resolves.toEqual({ output: "ok", problems: [] });
    expect(worker.terminated).toBe(false);
  });

  it("reports a worker that cannot start", async () => {
    const client = new FormatClient(() => {
      throw new Error("no workers here");
    }, 1000);
    await expect(client.format(request)).rejects.toThrow("the citation engine stopped unexpectedly");
  });
});
