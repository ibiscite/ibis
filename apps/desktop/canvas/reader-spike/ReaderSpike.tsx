// The reader spike's canvas board (#1252). Not production UI: it proves that
// PDF.js renders a synthetic PDF with a text layer in the app's front end and
// that page numbers keep their source. The panel's design is in docs/design.md.
import { useEffect, useRef, useState } from "react";
import type { PDFDocumentProxy } from "pdfjs-dist/legacy/build/pdf.mjs";
import { indicator, sourceWords, type PageNumber } from "./pageNumbers";
import { ReaderSession, type Failure } from "./session";
import { brokenPdf, LABELLED, SPECS, syntheticPdf } from "./synthetic-pdf";
import "./reader-spike.css";

type FileRow = { name: string; title: string; pages: PageNumber[] | null; failure: Failure | null };
const FAILURE_WORDS: Record<Failure, string> = {
  "too-large": "Too large to open here", encrypted: "Password-protected", damaged: "Damaged: PDF.js refused it",
  "timed-out": "Took too long to open", unreadable: "Couldn't read the file",
};
const SHOWN_PAGE = 8;

export function ReaderSpike() {
  const pageHost = useRef<HTMLDivElement>(null);
  const [shown, setShown] = useState<PageNumber | null>(null);
  const [files, setFiles] = useState<FileRow[]>([]);
  const [engine, setEngine] = useState("");
  useEffect(() => {
    let live = true;
    const cancels: (() => void)[] = [];
    void (async () => {
      const pdfjs = await import("./pdfjs");
      if (!live) return;
      setEngine(`PDF.js ${pdfjs.pdfjsVersion}, legacy build, own worker, bundled fonts, no network`);
      // One session per board: each file is one load with its own identity,
      // as if the person opened each in turn.
      const session = new ReaderSession<PDFDocumentProxy>(pdfjs.pdfjsEngine);
      const rows: FileRow[] = [];
      for (const spec of SPECS) {
        const outcome = await session.open(`synthetic:${spec.name}`, async () => syntheticPdf(spec));
        if (!live) return;
        if (outcome.kind === "stale") continue;
        rows.push({ name: spec.name, title: spec.title, pages: outcome.kind === "opened" ? outcome.pages : null, failure: outcome.kind === "failed" ? outcome.failure : null });
      }
      const broken = await session.open("synthetic:broken.pdf", async () => brokenPdf());
      if (!live) return;
      rows.push({ name: "broken.pdf", title: "Invented bytes that are not a PDF", pages: null, failure: broken.kind === "failed" ? broken.failure : null });
      setFiles(rows);
      // Now the page a person would be looking at.
      const opened = await session.open(`synthetic:${LABELLED.name}`, async () => syntheticPdf(LABELLED));
      if (!live || opened.kind !== "opened" || !pageHost.current) return;
      const page = await opened.document.getPage(SHOWN_PAGE);
      if (!live || session.current !== opened.load) return;
      const rendered = pdfjs.renderPage(page, pageHost.current, 0.9);
      cancels.push(rendered.cancel, () => session.close(), pdfjs.endWorker);
      await rendered.done;
      if (!live) return;
      setShown(opened.pages[SHOWN_PAGE - 1]!);
      document.documentElement.dataset.readerSpike = "ready";
    })().catch((error: unknown) => {
      document.documentElement.dataset.readerSpike = `error: ${error instanceof Error ? error.message : String(error)}`;
    });
    return () => { live = false; for (const cancel of cancels) cancel(); delete document.documentElement.dataset.readerSpike; };
  }, []);
  return <div className="reader-spike">
    <header className="reader-spike-head">
      <strong>Reader spike (#1252)</strong>
      <span>{engine || "Loading PDF.js…"}</span>
    </header>
    <div className="reader-spike-body">
      <section className="reader-spike-document" aria-label={shown ? indicator(shown) : "PDF page"}>
        <div className="reader-spike-sheet" ref={pageHost} lang="en" />
        <p className="reader-spike-indicator" aria-live="polite">{shown ? indicator(shown) : "Opening…"}</p>
        <p className="reader-spike-source">{shown ? sourceWords(shown) : ""}</p>
      </section>
      <section className="reader-spike-files" aria-label="Page numbers in each synthetic PDF">
        {files.map((file) => <table key={file.name}>
          <caption><code>{file.name}</code> {file.title}</caption>
          <thead><tr><th scope="col">PDF page</th><th scope="col">Cite as</th><th scope="col">Where from</th></tr></thead>
          <tbody>{file.pages
            ? file.pages.map((page) => <tr key={page.physical} data-current={file.name === LABELLED.name && page.physical === SHOWN_PAGE || undefined}>
              <td>{page.physical}</td><td>{page.printed ?? "Unknown"}</td><td>{sourceWords(page)}</td></tr>)
            : <tr><td colSpan={3}>{file.failure ? FAILURE_WORDS[file.failure] : "Not opened"}</td></tr>}</tbody>
        </table>)}
      </section>
    </div>
  </div>;
}
