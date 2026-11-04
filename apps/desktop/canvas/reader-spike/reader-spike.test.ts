// @vitest-environment node
// The reader spike's executable scenarios (#1252): the real PDF.js reads the
// synthetic files' page labels, and the load owner keeps stale results out.
import { describe, expect, test } from "vitest";
import { getDocument, GlobalWorkerOptions } from "pdfjs-dist/legacy/build/pdf.mjs";
import { indicator, pageNumbers, physicalForLabel, sourceWords } from "./pageNumbers";
import { MAX_PDF_BYTES, ReaderSession, type Engine, type EngineDocument } from "./session";
import { brokenPdf, LABELLED, MALFORMED, PARTIAL, syntheticPdf, UNLABELLED } from "./synthetic-pdf";

GlobalWorkerOptions.workerSrc = new URL("../../node_modules/pdfjs-dist/legacy/build/pdf.worker.mjs", import.meta.url).href;

async function labelsOf(bytes: Uint8Array) {
  const task = getDocument({ data: bytes, enableXfa: false, verbosity: 0 });
  try {
    const document = await task.promise;
    return { count: document.numPages, labels: await document.getPageLabels() };
  } finally { await task.destroy(); }
}

describe("page labels through PDF.js", () => {
  test("Roman front matter, an offprint run, a plate and a gap", async () => {
    const { count, labels } = await labelsOf(syntheticPdf(LABELLED));
    const pages = pageNumbers(labels, count);
    expect(pages.map((page) => page.printed)).toEqual(LABELLED.printed);
    expect(pages.every((page) => page.source === "pdf-page-labels")).toBe(true);
    expect(indicator(pages[7]!)).toBe("PDF page 8 · cite p. 416");
    expect(indicator(pages[2]!)).toBe("PDF page 3 · cite p. iii");
    // The gap is real: 417 is printed nowhere, so no page is offered for it.
    expect(physicalForLabel(pages, "417")).toEqual([]);
    expect(physicalForLabel(pages, "419")).toEqual([10]);
  });

  test("no page labels: the printed page is unknown, never the physical index", async () => {
    const { count, labels } = await labelsOf(syntheticPdf(UNLABELLED));
    expect(labels).toBeNull();
    const pages = pageNumbers(labels, count);
    expect(pages.map((page) => [page.printed, page.source])).toEqual([[null, "none"], [null, "none"], [null, "none"]]);
    expect(indicator(pages[0]!)).toBe("PDF page 1 · Citation page unknown");
    expect(sourceWords(pages[0]!)).toBe("This PDF gives no readable page labels");
  });

  test("labels that start late leave earlier pages blank; a repeated label is ambiguous", async () => {
    const { count, labels } = await labelsOf(syntheticPdf(PARTIAL));
    const pages = pageNumbers(labels, count);
    expect(pages.map((page) => page.printed)).toEqual([null, null, "5", "6", "5"]);
    expect(pages[0]!.source).toBe("pdf-empty-label");
    expect(physicalForLabel(pages, "5")).toEqual([3, 5]);
    expect(sourceWords(pages[2]!)).toBe("From the PDF's page labels; also on PDF page 5");
  });

  test("malformed labels count as none for the whole file", async () => {
    const { count, labels } = await labelsOf(syntheticPdf(MALFORMED));
    expect(labels).toBeNull();
    expect(pageNumbers(labels, count).every((page) => page.printed === null)).toBe(true);
  });

  test("a damaged file is refused by PDF.js, not half-read", async () => {
    await expect(labelsOf(brokenPdf())).rejects.toMatchObject({ name: "InvalidPDFException" });
  });

  test("a label list of the wrong length is not evidence", () => {
    expect(pageNumbers(["1", "2"], 3).map((page) => page.source)).toEqual(["none", "none", "none"]);
  });
});

type Fake = EngineDocument & { name: string };
function deferred<T>() {
  let resolve!: (value: T) => void, reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
function fakeEngine() {
  const opened: { bytes: Uint8Array; done: ReturnType<typeof deferred<Fake>>; destroyed: boolean }[] = [];
  const engine: Engine<Fake> = {
    open(bytes) {
      const entry = { bytes, done: deferred<Fake>(), destroyed: false };
      opened.push(entry);
      return { promise: entry.done.promise, destroy: async () => { entry.destroyed = true; } };
    },
  };
  const doc = (name: string, labels: string[] | null = null): Fake => ({
    name, numPages: labels?.length ?? 1, getPageLabels: async () => labels,
  });
  return { engine, opened, doc };
}
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("one owner per load", () => {
  test("success reports the document and its page numbers", async () => {
    const { engine, opened, doc } = fakeEngine();
    const session = new ReaderSession(engine);
    const result = session.open("attachment-a", async () => new Uint8Array(4));
    await tick();
    opened[0]!.done.resolve(doc("a", ["i", "1"]));
    const outcome = await result;
    expect(outcome).toMatchObject({ kind: "opened", load: { id: 1, attachment: "attachment-a" } });
    expect(outcome.kind === "opened" && outcome.pages.map((page) => page.printed)).toEqual(["i", "1"]);
    expect(Object.isFrozen(session.current)).toBe(true);
  });

  test("refusal: a file over the size limit never reaches the engine", async () => {
    const { engine, opened } = fakeEngine();
    const session = new ReaderSession(engine);
    const huge = { byteLength: MAX_PDF_BYTES + 1 } as Uint8Array;
    expect(await session.open("big", async () => huge)).toMatchObject({ kind: "failed", failure: "too-large" });
    expect(opened).toHaveLength(0);
    expect(session.current).toBeNull();
  });

  test("failed I/O and an encrypted file each say what happened", async () => {
    const { engine, opened } = fakeEngine();
    const session = new ReaderSession(engine);
    expect(await session.open("gone", async () => { throw new Error("missing"); })).toMatchObject({ kind: "failed", failure: "unreadable" });
    const locked = session.open("locked", async () => new Uint8Array(4));
    await tick();
    opened[0]!.done.reject(Object.assign(new Error("No password given"), { name: "PasswordException" }));
    expect(await locked).toMatchObject({ kind: "failed", failure: "encrypted" });
  });

  test("delayed A after B: A's late document is destroyed and reports nothing", async () => {
    const { engine, opened, doc } = fakeEngine();
    const session = new ReaderSession(engine);
    const a = session.open("a", async () => new Uint8Array(4));
    await tick();
    const b = session.open("b", async () => new Uint8Array(4));
    await tick();
    expect(opened[0]!.destroyed).toBe(true);
    const lateB = doc("b"), lateA = doc("a");
    opened[1]!.done.resolve(lateB);
    expect(await b).toMatchObject({ kind: "opened", load: { attachment: "b" } });
    opened[0]!.done.resolve(lateA);
    expect(await a).toEqual({ kind: "stale" });
    expect(opened.map((entry) => entry.destroyed)).toEqual([true, false]);
    expect(session.current?.attachment).toBe("b");
  });

  test("close then reopen: the old read is aborted and its bytes are never opened", async () => {
    const { engine, opened, doc } = fakeEngine();
    const session = new ReaderSession(engine);
    const slow = deferred<Uint8Array>();
    let signal: AbortSignal | undefined;
    const first = session.open("a", async (s) => { signal = s; return slow.promise; });
    session.close();
    expect(signal?.aborted).toBe(true);
    const second = session.open("a", async () => new Uint8Array(4));
    await tick();
    slow.resolve(new Uint8Array(4));
    expect(await first).toEqual({ kind: "stale" });
    expect(opened).toHaveLength(1);
    opened[0]!.done.resolve(doc("a"));
    expect(await second).toMatchObject({ kind: "opened", load: { id: 2 } });
  });

  test("an open that takes too long fails once and frees what arrives later", async () => {
    const { engine, opened, doc } = fakeEngine();
    const session = new ReaderSession(engine, 20);
    const result = session.open("slow", async () => new Uint8Array(4));
    expect(await result).toMatchObject({ kind: "failed", failure: "timed-out" });
    expect(opened[0]!.destroyed).toBe(true);
    opened[0]!.done.resolve(doc("slow"));
    await tick();
    expect(session.current).toBeNull();
  });
});
