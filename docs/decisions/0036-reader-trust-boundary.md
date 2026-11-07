# 0036 The reader's trust boundary

2026-11-04 (maintainer): Ibis gets a full-quality, page-level PDF reader so that
nobody needs to open Zotero while writing. It comes forward and no longer waits
for citing from annotations. It is a read-only viewer for PDFs already in the
person's citation manager (a Zotero attachment or a `.bib` entry's `file` field),
to look at a source and choose a page. It does not replace Zotero: Ibis never
imports, stores, manages, downloads, edits or annotates a PDF, and Open in Zotero
stays. It opens in the shared inspector panel; while a PDF is open the list
folds away and the window grows once, taller and wider, to hold a readable page
(maintainer, 2026-11-04, decision 0038; this replaces "beside the list, widening
only to hold the PDF" of 2026-11-02). It is one priority among many;
design polish, shortcuts and configuration matter as much.

**Engine: PDF.js** (`pdfjs-dist`, legacy build, pinned exactly, behind one
adapter module), compared on 2026-11-04 with a spike on the branch
`claude/reader-boundary` (#1252):

| | PDF.js in the web view | pdfium (`pdfium-render`) | Each system's own (PDFKit, Windows.Data.Pdf, poppler) |
| --- | --- | --- | --- |
| Untrusted input | Memory-safe JavaScript in a worker, inside the web view's content process (sandboxed on macOS and Windows; not yet on Linux, see below). CVE-2024-4367, code execution through PDF.js's old `eval` font path, was fixed in 4.2.67 | A C++ parser inside the Ibis process, beside the engine, the clipboard and paste permissions; Chrome runs it only in a sandboxed renderer | Three C/C++ parsers (CoreGraphics' JBIG2 decoder was the FORCEDENTRY exploit, CVE-2021-30860); a native view inside the window also bypasses the one key listener and focus rules |
| Same on every system | One code base. Upstream supports the legacy build on Chrome 125+ (WebView2) and "mostly" on Safari 18+, and does not test WebKit, so Ibis tests it | Same pixels | Three renderers that differ |
| Text and screen readers | A DOM text layer that can be selected, found and read (the spike) | Text and glyph boxes; Ibis would rebuild the DOM layer | PDFKit is accessible; Windows.Data.Pdf has no text at all |
| Page labels | `getPageLabels()`: null for absent or malformed labels, empty for unlabelled pages, never guessed (the spike) | `FPDF_GetPageLabel` | PDFKit yes, Windows.Data.Pdf no, poppler yes |
| Size | A lazy chunk of 564 KiB gzipped (147 main, 382 worker, two fonts); about 1.8 MB in the installer with cMaps, image decoders and Foxit fonts. The picker grows by 0.7 KiB | About 5 to 7 MB of native library per system and architecture (not measured here) | Nothing on macOS or Windows; poppler and GLib on Linux |
| Memory | Worker and canvases only while open; ended on hide | In the app process | Managed by the system |
| Licence | Apache-2.0; bundled parts BSD (Foxit fonts, OpenJPEG, JBIG2 from PDFium, Adobe cMaps), MIT (qcms), CC0 (an ICC profile) | Crate MIT or Apache-2.0; the binary bundles FreeType, libjpeg-turbo, OpenJPEG and LCMS, which `cargo deny` cannot see | poppler is GPL (from Xpdf) and needs its own licence decision |

PDF.js is the only option that keeps untrusted parsing out of the Ibis process,
draws the same pages on every system and gives screen readers real text.

- **Who decided.** The first paragraph is the maintainer's direction, as are the
  Linux hold and the answer on `'wasm-unsafe-eval'` below (2026-11-04: take the
  security review's recommended options). The engine and the other rules are
  engineering choices, changed by pull request with a review of the final head.
- **Bytes, not paths.** One command, granted only to the picker window (0024),
  takes an opaque attachment id the engine issued for the current library
  generation (#1208) and a load id, and returns the bytes as a raw IPC response
  (`tauri::ipc::Response`, an `ArrayBuffer`), which PDF.js transfers to its
  worker. The engine finds the file; no path, file URL, `asset:` URL or
  `convertFileSrc` reaches the web view, an error or a log. The read runs on the
  bounded file-I/O budget (`ibis-sources/src/file/io.rs`, `bounded`), keyed by the
  load id. `invoke` takes no `AbortSignal`, so a second command cancels a load id
  on the Rust side, and a cancelled or overtaken read sends nothing; an `open()`
  stuck in the kernel holds one budget slot, never the picker, and the load
  fails at 15 s. The file is opened first and the handle checked: a regular file
  (`fstat`), at most 256 MiB, placeholder flags rechecked on the handle just
  before reading (a dataless file on macOS, read with
  `IOPOL_MATERIALIZE_DATALESS_FILES_OFF` on the reading thread so it never
  downloads; recall-on-open or recall-on-data-access on Windows), then `%PDF-` in
  the first 1,024 bytes. Network file systems and Linux FUSE mounts, where cloud
  clients live, are refused as not on this computer, since a read there can block
  or download. If the window's IPC has fallen back to `postMessage`, which would
  carry the bytes as a JSON array of numbers, the command refuses. Bytes stay in
  memory only while the PDF is open: never written, cached or sent anywhere.
- **Ibis owns the worker.** Ibis creates a dedicated `Worker` from a same-origin
  script and always hands it to PDF.js as `port`: never `workerSrc`, whose failure
  silently falls back to parsing on the main thread, and never a `blob:` worker.
  Tauri 2.12.5 attaches the CSP only to `.html` assets, so Ibis sets the app's CSP
  on the worker script's response itself (`on_web_resource_request` or a header
  hook), and a test on each web view shows `fetch()` and `WebAssembly.compile`
  refused inside the worker. Closing the reader or hiding the picker destroys the
  task and then calls `terminate()`, because PDF.js terminates only workers it
  made; a test shows no live worker after hiding.
- **A viewer of bytes.** `getDocument({ data })` only: no URL, range requests,
  streaming or auto-fetch. No scripting (neither `pdf.sandbox` nor the QuickJS
  wasm ships), no XFA and no annotation layer, so links, forms, JavaScript,
  launch, submit and embedded files do nothing; stored annotation appearances are
  drawn as pictures. Standard fonts and cMaps are bundled and served by an
  in-memory factory. `useWasm: false` is set explicitly; the two JavaScript image
  decoders (`openjpeg_nowasm_fallback.js`, `jbig2_nowasm_fallback.js`) ship as
  same-origin static files with `wasmUrl` pointing at them, because PDF.js loads
  them with `import()`. Nothing else is fetched. The CSP gains neither
  `unsafe-eval` nor `'wasm-unsafe-eval'` (maintainer: no), which would enable the
  wasm decoders and compiling PDF functions to wasm. PDF.js 6.4.299 has no
  reachable `eval` or `new Function`; the legacy build's core-js keeps an
  unreachable `Function('return this')` fallback. Hardening, kept if measurement
  allows: `disableFontFace: true` (glyphs drawn as paths) and
  `isImageDecoderSupported: false`.
- **What remains exposed.** Embedded fonts (unless drawn as paths) and JPEG
  images still reach the web view's native decoders. A compromised content
  process holds every grant the picker window has. A Flate bomb or a huge page
  can exhaust the content process and crash the picker. On Linux wry 0.57 never
  enables WebKitGTK's sandbox and the AppImage bundles its own WebKitGTK, so
  **the reader stays off on Linux until Ibis enables the WebKitGTK sandbox before
  the first web view** (maintainer). After a web-content crash with the reader
  open, Ibis does not reopen that file, by content hash, for the rest of the
  session. To evaluate: the reader in a child web view whose only grant is the
  bytes command.
- **Limits.** At most 256 MiB and 10,000 pages a file; opening, bytes to first
  page drawn, takes at most 15 s, then fails with Try Again and Open in Zotero. Images up to
  100 megapixels (`maxImageSize`) within a per-page budget of decoded image bytes;
  a page that loses an image says "Part of this page is too large to draw" and is
  never shown blank as if that were its content. Ibis caps each page canvas at 16
  megapixels and enlarges by CSS beyond it (`canvasMaxAreaInBytes` only governs
  downscaling inside the worker). The page list is virtualised with a minimum
  laid-out page height; only pages in view and one either side are drawn, and
  renders that leave the view are cancelled and their canvases and text layers
  removed.
- **One owner per load.** Each open belongs to the inspector's reader session and
  has an immutable identity: load id, attachment id and library generation.
  Opening another source, closing the reader or the panel, hiding the picker, a
  reload that changes the attachment, or a newer open cancels the read by load
  id and destroys the PDF.js task. A completion that is no longer current is
  destroyed and reports nothing; a page render carries its load id and page and
  is cancelled, never drawn, when stale. The Page field the reader fills belongs
  to the citation item that opened it; if that item has left the shelf, nothing
  is filled.
- **Memory.** The reader is a dynamic import loaded on first open;
  `scripts/picker-budget.mjs` counts only the picker's static graph, so startup
  stays at today's 222 KiB of 400 KiB (the spike measured +0.7 KiB for the import
  itself). Hiding the picker or closing the reader ends the document and the
  worker; only the parsed module code stays. A release check opens an invented
  300-page PDF, hides the picker and requires the hidden 200 MiB limit within 2 s.
- **Page numbers.** The physical index, the printed label and an annotation's
  label stay apart (architecture, **Reader**). A printed page comes from a
  non-empty `/PageLabels` entry, a Zotero annotation's label or an offset the
  person confirms; otherwise it is unknown, never the physical index. A label is
  citable only if, after NFC, it has at most 16 characters of digits, letters
  (Roman numerals included), `-` and `.`, and no control or format characters
  (Cc, Cf); any other label is shown but counts as unknown. Annotation labels pass
  the same test, and one that disagrees with `/PageLabels` makes the page
  ambiguous. An ambiguous page, or a label on several pages, asks. When the record
  has a page range and the label falls outside it, Page waits for the person to
  confirm and the indicator shows both. pypdf, a widely used parser, fills in
  "1, 2, 3" for a file with no labels and "3, 4" for pages a label range leaves
  blank: the silent guess Ibis refuses.
- **Notes HTML never reaches the DOM as HTML.** The engine parses Zotero note
  HTML with html5ever (already in `Cargo.lock` through Tauri) into a small typed
  tree: paragraphs, headings, lists, quotations, emphasis, strong, sub- and
  superscript, code and text, at most 1 MiB and 32 levels deep. Links keep their
  words and lose their target; images, styles, scripts, frames, forms and every
  URL are dropped. React renders the tree as elements and text nodes, never
  `innerHTML`. Annotation comments and PDF text are plain text (PDF.js's text
  layer sets `textContent`).
- **Still to prove.** The spike ran in Chromium 153, WebView2's engine. Before
  the reader ships: the same scenario in WKWebView on each supported macOS and
  WebView2 on Windows; the worker CSP test and the no-live-worker test on each web
  view; the hardening options measured. The legacy build needs Safari 18's engine
  or later, so the reader checks the web view's engine at run time and is
  otherwise unavailable with that reason; macOS 12 (last Safari 17.6) fails it.
- **Licences.** `cargo deny` does not see JavaScript; PDF.js and each bundled part
  are listed in `NOTICE.md` by hand and pinned in `pnpm-lock.yaml`. PDF.js also
  ships Liberation Sans (GPL-2.0 with a font exception, under Red Hat's terms) as
  its Helvetica substitute; Ibis leaves it out until the maintainer decides, and
  PDFs that do not embed Helvetica use the system's sans-serif.
