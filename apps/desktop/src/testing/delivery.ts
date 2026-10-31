/**
 * Stateful IPC fake for whole-picker scenarios; no OS clipboard or input access.
 * Render App normally after deliveryHarness(). Every search/preview/copy waits
 * for its queue's pending[n].complete(value), inside React act(). Searches and
 * previews take invented engine fixtures; copy validates the returned ticket/hash
 * and writes that exact preview to the clipboard, never a supplied copy receipt.
 * Like macOS, its successful write captures the current app and presentation;
 * App must suppress stale completion feedback even after that native write.
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
type Capture = { window: number | null; surface: number | null; reason: string | null };
type Proof = DeliveryRequest & Capture & { delivery: number; sequence: number; presentation: number; app: string; epoch: number; written: number };

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
  const documents = new Map<number, Pick<typeof document, "text" | "selection">>([
    [100, document],
    [200, { text: "Second: [citation].", selection: [8, 18] }],
  ]);
  const target = { window: 100 as number | null, verified: true, available: true,
    surface: null as number | null, focusedPid: 42, focusAvailable: true, focusTimedOut: false };
  const authorizations = { postEvent: true, accessibility: true };
  const lifecycle = { epoch: 1, elapsed: 0, available: true };
  // Detached to keep this second window out of the picker's DOM and focus scope.
  const receiptRoot = globalThis.document.createElement("div");
  const showReceipt = renderNotice(receiptRoot, "macos");
  let presentation = 0;
  let visible = true;
  let nextDelivery = 0;
  let proof: Proof | null = null;
  let latestPreview = 0;
  let activeTicket: string | null = null;
  let latestNotice = 0;
  let captured: Capture | null = null;
  const capture = (): Capture => {
    const reason = !authorizations.postEvent ? "no-permission" : !authorizations.accessibility ? "accessibility-required"
      : target.window === null || !target.focusAvailable || target.focusTimedOut || target.focusedPid !== 42 ? "paste-unavailable" : null;
    return { window: reason ? null : target.window, surface: reason ? null : target.surface ?? target.window, reason };
  };

  const deliver = ({ text, delivery }: DeliveryRequest): Delivered => {
    // Identity precedes consumption: a stale worker must leave a newer proof.
    const owned = proof?.delivery === delivery && proof?.text === text ? proof : null;
    if (!owned) return { outcome: "copied", reason: null };
    proof = null;
    if (!target.verified) return { outcome: "copied", reason: "unknown-app" };
    if (owned.reason) return { outcome: "copied", reason: owned.reason };
    if (owned.window === null) return { outcome: "copied", reason: "paste-unavailable" };
    if (!lifecycle.available) return { outcome: "copied", reason: "paste-unavailable" };
    if (owned.epoch !== lifecycle.epoch || lifecycle.elapsed - owned.written >= 1500) return { outcome: "copied", reason: "paste-expired" };
    if (!authorizations.postEvent) return { outcome: "copied", reason: "no-permission" };
    if (!authorizations.accessibility) return { outcome: "copied", reason: "accessibility-required" };
    if (!target.focusAvailable || target.focusTimedOut) return { outcome: "copied", reason: "paste-unavailable" };
    if (target.focusedPid !== 42) return { outcome: "copied", reason: "target-changed" };
    if (clipboard.owner !== "ibis" || owned.sequence !== clipboard.sequence) return { outcome: "copied", reason: "clipboard-changed" };
    if (!target.available || owned.window !== target.window || owned.surface !== (target.surface ?? target.window)) return { outcome: "copied", reason: "target-changed" };
    if (visible || owned.presentation !== presentation || clipboard.owner !== "ibis"
      || owned.sequence !== clipboard.sequence || owned.app !== document.app) return { outcome: "copied", reason: null };
    const destination = owned.surface === null ? undefined : documents.get(owned.surface);
    if (!destination) return { outcome: "copied", reason: "target-changed" };
    const [start, end] = destination.selection;
    destination.text = destination.text.slice(0, start) + clipboard.text + destination.text.slice(end);
    destination.selection = [start + clipboard.text.length, start + clipboard.text.length];
    document.pastes.push({ app: owned.app, text: clipboard.text, start, end, delivery: owned.delivery });
    return { outcome: "pasted", app: owned.app };
  };

  const calls = backend({
    "plugin:window|is_visible": () => visible,
    picker_destination: () => {
      captured ??= capture();
      return { app: document.app, app_id: "example.writer", automatic: { key: "pandoc" }, remembered: null, unreadable: false, can_paste: captured.reason === null, output_question: null };
    },
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
      return copy.enqueue(args, () => {
        const { preview } = prepared(args);
        if (args.ticket !== activeTicket) throw { code: "snapshot_changed" };
        Object.assign(clipboard, { text: preview.text, html: preview.html, owner: "ibis", sequence: clipboard.sequence + 1 });
        const token = ++nextDelivery;
        proof = { ...(captured ?? capture()), presentation, app: document.app, text: preview.text, delivery: token, sequence: clipboard.sequence, epoch: lifecycle.epoch, written: lifecycle.elapsed };
        return { text: clipboard.text, warnings: preview.warnings, delivery: token };
      });
    },
    hide_picker: async () => { visible = false; await emit("picker:hidden"); return null; },
    show_copied_notice: (args) => {
      const request = args as unknown as DeliveryRequest;
      const noticeOwner = ++latestNotice;
      return delivery.enqueue(request, () => deliver(request))
        .then((delivered) => notice.enqueue({ text: request.text, delivered }, () => {
          if (noticeOwner === latestNotice) showReceipt({ text: request.text, delivered });
          return delivered;
        }));
    },
  });

  return {
    search, preview, copy, delivery, notice, calls, clipboard, document, documents, target, authorizations, lifecycle, receipt: receiptRoot,
    get visible() { return visible; },
    async reopen() {
      presentation += 1;
      proof = null;
      captured = capture();
      visible = true;
      await emit("picker:shown");
    },
    copyElsewhere(text: string) {
      Object.assign(clipboard, { text, html: null, owner: "external", sequence: clipboard.sequence + 1 });
    },
  };
}
