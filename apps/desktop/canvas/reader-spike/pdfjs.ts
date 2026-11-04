// The PDF.js adapter for the reader spike (#1252), loaded only when the reader
// opens. Bytes in, pages out: no URL, no path, no fetch, no scripting, no
// forms, no links. Every resource PDF.js may ask for is bundled and served
// from memory; anything else is refused.
import {
  AnnotationMode, getDocument, PDFWorker, TextLayer, version,
  type PDFDocumentProxy, type PDFPageProxy, type RenderTask,
} from "pdfjs-dist/legacy/build/pdf.mjs";
import workerSource from "pdfjs-dist/legacy/build/pdf.worker.min.mjs?raw";
import foxitSerif from "pdfjs-dist/standard_fonts/FoxitSerif.pfb?inline";
import foxitSerifBold from "pdfjs-dist/standard_fonts/FoxitSerifBold.pfb?inline";
import type { Engine } from "./session";

export const pdfjsVersion = version;

const bundled: Record<string, string> = { "FoxitSerif.pfb": foxitSerif, "FoxitSerifBold.pfb": foxitSerifBold };
const decode = (dataUrl: string) => Uint8Array.from(atob(dataUrl.slice(dataUrl.indexOf(",") + 1)), (c) => c.charCodeAt(0));

/** Serves only bundled standard fonts; a cMap or wasm request fails closed. */
class BundledData {
  requests: string[] = [];
  async fetch({ kind, filename }: { kind: string; filename: string }): Promise<Uint8Array> {
    const data = kind === "standardFontDataUrl" ? bundled[filename] : undefined;
    if (!data) throw new Error(`Not bundled in the spike: ${kind} ${filename}`);
    return decode(data);
  }
}

// The app would load the worker from its own origin (CSP worker-src 'self');
// the canvas has no server, so the same bytes start from a blob URL here.
let worker: PDFWorker | null = null;
function sharedWorker(): PDFWorker {
  if (worker && !worker.destroyed) return worker;
  const url = URL.createObjectURL(new Blob([workerSource], { type: "text/javascript" }));
  worker = new PDFWorker({ port: new Worker(url, { type: "module" }) as unknown as null });
  return worker;
}

/** Closing the reader (or hiding the picker) ends the worker and its memory. */
export function endWorker(): void { worker?.destroy(); worker = null; }

export const pdfjsEngine: Engine<PDFDocumentProxy> = {
  open(bytes) {
    const task = getDocument({
      data: bytes,
      worker: sharedWorker(),
      // Never a URL: no range requests, streaming or auto-fetch.
      disableRange: true, disableStream: true, disableAutoFetch: true,
      enableXfa: false,
      // Identical glyphs everywhere: bundled standard fonts, not the system's.
      useSystemFonts: false, standardFontDataUrl: "ibis-bundled/", useWorkerFetch: false,
      BinaryDataFactory: BundledData as never,
      // Bounded decoding: images over 16 megapixels are skipped, not decoded.
      maxImageSize: 4096 * 4096,
      canvasMaxAreaInBytes: 4096 * 4096 * 4,
      verbosity: 0,
    });
    return { promise: task.promise, destroy: () => task.destroy() };
  },
};

export type RenderedPage = { cancel(): void; done: Promise<void> };

/** One page: the canvas picture and, above it, the selectable text layer. */
export function renderPage(page: PDFPageProxy, host: HTMLElement, cssScale: number): RenderedPage {
  const viewport = page.getViewport({ scale: cssScale });
  const ratio = window.devicePixelRatio || 1;
  host.replaceChildren();
  host.style.setProperty("--total-scale-factor", String(cssScale));
  host.style.setProperty("--scale-round-x", "1px");
  host.style.setProperty("--scale-round-y", "1px");
  host.style.width = `${Math.floor(viewport.width)}px`;
  host.style.height = `${Math.floor(viewport.height)}px`;
  const canvas = host.appendChild(document.createElement("canvas"));
  canvas.width = Math.floor(viewport.width * ratio);
  canvas.height = Math.floor(viewport.height * ratio);
  canvas.style.width = `${Math.floor(viewport.width)}px`;
  canvas.style.height = `${Math.floor(viewport.height)}px`;
  // The picture is decorative: the text layer is what a screen reader reads.
  canvas.setAttribute("aria-hidden", "true");
  const text = host.appendChild(document.createElement("div"));
  text.className = "textLayer";
  const render: RenderTask = page.render({
    canvas, viewport, ...(ratio === 1 ? {} : { transform: [ratio, 0, 0, ratio, 0, 0] }),
    // Stored appearances are drawn; nothing is interactive (no forms, no links).
    annotationMode: AnnotationMode.ENABLE,
  });
  const layer = new TextLayer({ textContentSource: page.streamTextContent(), container: text, viewport });
  const done = Promise.all([render.promise, layer.render()]).then(() => {});
  return { done, cancel: () => { render.cancel(); layer.cancel(); } };
}
