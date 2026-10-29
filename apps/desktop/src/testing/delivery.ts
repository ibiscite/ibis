/**
 * Stateful IPC fake for whole-picker scenarios; no OS clipboard or input access.
 * Render App normally after deliveryHarness(). Every search/preview/copy waits
 * for its queue's pending[n].complete(value), inside React act(). Searches and
 * previews take invented engine fixtures; copy validates the returned ticket/hash
 * and writes that exact preview to the clipboard, never a supplied copy receipt.
 *
 * show_copied_notice queues delivery first: completing it consumes a matching
 * one-use proof, checks clipboard ownership/sequence and picker presentation,
 * then pastes at the fake document's selection. Completing the notice renders
 * the real receipt UI and returns that derived outcome to App. This separates
 * native effects from the response, so tests can reopen between them. Complete
 * pending jobs in any order; completing a job twice throws. Inspect calls for
 * counts, document.pastes for actual deliveries, and receipt for visible text.
 *
 * This models the interface contract, not platform permission/input APIs, native
 * deadlines, engine formatting or hashing. Their Rust tests remain necessary.
 */
import { emit } from "@tauri-apps/api/event";
import type { Copied, Delivered, FilePreview, NoticeMessage, SearchQuery, SearchReply } from "../bindings";
import { renderNotice } from "../copyFeedback";
import { backend, prepared, receipt, type PreviewRequest } from "./app";

type Job<Args, Input> = {
  args: Args;
  pending: boolean;
  complete: (value: Input) => void;
  fail: (error: unknown) => void;
};

/** A manual worker queue: no timers, automatic drain or arbitrary result stubs. */
class Work<Args, Input = void, Output = void> {
  readonly calls: Job<Args, Input>[] = [];
  get pending() { return this.calls.filter((job) => job.pending); }

  enqueue(args: Args, finish: (value: Input) => Output): Promise<Output> {
    return new Promise((resolve, reject) => {
      const settle = () => {
        if (!job.pending) throw new Error("This fake worker has already completed.");
        job.pending = false;
      };
      const job: Job<Args, Input> = {
        args, pending: true,
        complete: (value) => {
          settle();
          try { resolve(finish(value)); } catch (error) { reject(error); }
        },
        fail: (error) => { settle(); reject(error); },
      };
      this.calls.push(job);
    });
  }
}

type PreviewContent = Pick<FilePreview, "text"> & Partial<Pick<FilePreview, "html" | "warnings">>;
type DeliveryRequest = { text: string; delivery: number | null };
type Proof = DeliveryRequest & { delivery: number; sequence: number; presentation: number; app: string };

export function deliveryHarness(text = "Draft: [citation].", selection: [number, number] = [7, 17]) {
  const search = new Work<SearchQuery, SearchReply, SearchReply>();
  const preview = new Work<PreviewRequest, PreviewContent, FilePreview>();
  const copy = new Work<{ ticket: string; contentHash: string }, void, Copied>();
  const delivery = new Work<DeliveryRequest, void, Delivered>();
  const notice = new Work<NoticeMessage, void, Delivered>();
  const clipboard = { text: "", html: null as string | null, owner: null as "ibis" | "external" | null, sequence: 0 };
  const document = {
    app: "Example Writer", text, selection,
    pastes: [] as { app: string; text: string; start: number; end: number; delivery: number }[],
  };
  // Detached to keep this second window out of the picker's DOM and focus scope.
  const receiptRoot = globalThis.document.createElement("div");
  const showReceipt = renderNotice(receiptRoot, "macos");
  let presentation = 0;
  let visible = true;
  let nextDelivery = 0;
  let proof: Proof | null = null;
  let latestPreview = 0;
  let activeTicket: string | null = null;

  const deliver = ({ text, delivery }: DeliveryRequest): Delivered => {
    // Identity precedes consumption: a stale worker must leave a newer proof.
    const owned = proof?.delivery === delivery && proof?.text === text ? proof : null;
    if (!owned) return { outcome: "copied", reason: null };
    proof = null;
    if (visible || owned.presentation !== presentation || clipboard.owner !== "ibis"
      || owned.sequence !== clipboard.sequence || owned.app !== document.app) return { outcome: "copied", reason: null };
    const [start, end] = document.selection;
    document.text = document.text.slice(0, start) + clipboard.text + document.text.slice(end);
    document.selection = [start + clipboard.text.length, start + clipboard.text.length];
    document.pastes.push({ app: owned.app, text: clipboard.text, start, end, delivery: owned.delivery });
    return { outcome: "pasted", app: owned.app };
  };

  const calls = backend({
    "plugin:window|is_visible": () => visible,
    picker_destination: () => ({ app: document.app, app_id: "example.writer", automatic: { key: "pandoc" }, remembered: null, unreadable: false, output_question: null }),
    search: ({ query }) => search.enqueue(query, (reply) => reply),
    preview_file_citation: (args) => {
      const request = ++latestPreview;
      return preview.enqueue(args, (content) => {
        const value = receipt(content.text, content.html ?? null, content.warnings ?? []);
        if (request === latestPreview) activeTicket = value.ticket;
        return value;
      });
    },
    copy_file_citation: (args) => {
      const captured = { presentation, app: document.app };
      return copy.enqueue(args, () => {
        const { preview } = prepared(args);
        if (args.ticket !== activeTicket) throw { code: "snapshot_changed" };
        Object.assign(clipboard, { text: preview.text, html: preview.html, owner: "ibis", sequence: clipboard.sequence + 1 });
        const token = ++nextDelivery;
        proof = { ...captured, text: preview.text, delivery: token, sequence: clipboard.sequence };
        return { text: clipboard.text, warnings: preview.warnings, delivery: token };
      });
    },
    hide_picker: async () => { visible = false; await emit("picker:hidden"); return null; },
    show_copied_notice: (args) => {
      const request = args as unknown as DeliveryRequest;
      return delivery.enqueue(request, () => deliver(request))
        .then((delivered) => notice.enqueue({ text: request.text, delivered }, () => {
          showReceipt({ text: request.text, delivered });
          return delivered;
        }));
    },
  });

  return {
    search, preview, copy, delivery, notice, calls, clipboard, document, receipt: receiptRoot,
    get visible() { return visible; },
    async reopen() {
      presentation += 1;
      proof = null;
      visible = true;
      await emit("picker:shown");
    },
    copyElsewhere(text: string) {
      Object.assign(clipboard, { text, html: null, owner: "external", sequence: clipboard.sequence + 1 });
    },
  };
}
