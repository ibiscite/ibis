# 0036 The reader's trust boundary

The planned reader is read-only: view an existing attachment (Zotero, a
`.bib` file's link or a folder of PDFs) and choose a citation page without
leaving Ibis. It never imports, downloads, stores, edits or annotates PDFs.
Open in Zotero stays available. It ships on macOS, Windows and Linux.

Amended 2026-11-25 ([0059](0059-works-with-nothing-better-with-more.md)): the
reader moves out of the picker into a dedicated `reader` window, and PDF.js
only paints. The engine's child extractor
([0055](0055-agent-layers-full-text-writes-remote.md)) is the only source of
page text, page labels and Anchors; search inside a document is `find_in`. The
picker's grants never sit beside untrusted PDF parsing.

PDF.js (`pdfjs-dist`, exactly pinned to a release no older than 6.2.108,
behind one adapter) parses in a worker of the reader window, draws pages with a
selectable text layer and uses the same renderer on each system. Native PDF
parsers would put C/C++ parsing beside Ibis's library, clipboard and paste
permissions or require separate viewers. The PDF adapter is not yet available
in the running app.

## The reader window

The reader window is granted three calls and nothing else: bytes by opaque
attachment ID (with its cancel), "selection made" (physical page index and
quoted text) and close. Its CSP allows no network and no eval. Qualification
proves the window gets its own content process; if it does not, the reader gets
its own origin.

A returned selection is untrusted. The engine matches it against its own page
text after normalizing (NFKC, ligatures, hyphenation, whitespace). On a match
the quote comes from the engine's text; otherwise the Anchor keeps the page and
the quote is refused with a reason.

## Bytes and lifetime

A reader-only IPC command accepts an engine-issued opaque attachment ID for
the current library generation and a load ID. The engine resolves the file;
no path or file URL reaches the webview, logs or errors. Bytes use raw
`ArrayBuffer` IPC; JSON-array transport refuses. A separate command cancels
by load ID. Reads use the bounded file-I/O pool and a 15 s deadline, so a stuck
kernel read occupies one slot without blocking the picker.

Open the handle first, then require a regular local file of at most 256 MiB,
recheck placeholder flags and find `%PDF-` within its first 1,024 bytes.
macOS reads disable dataless-file materialization; Windows refuses recall-on-
open/data-access files. Network filesystems and Linux FUSE mounts refuse.
Bytes remain only in memory while open.

Each load binds attachment, library generation and reader opening. Replacing
the source, closing, hiding or a changed attachment cancels the read and
destroys the task. Stale completions publish nothing. Page renders own that
load ID and page; leaving view cancels and removes canvas/text layers. A page
choice edits only the citation item that opened it, while that item remains
selected.

## Worker and content restrictions

Ibis creates and owns a same-origin worker and supplies its port to PDF.js.
It never uses a blob worker or PDF.js's main-thread parsing fallback. The
worker script response receives the app CSP; native webview tests must prove
`fetch` and WebAssembly compilation refuse there. Closing or hiding destroys
the PDF task and terminates Ibis's worker.

Use `getDocument({ data })` only: no URL, range requests, streaming, auto-fetch,
scripting, XFA or annotation layer. Links, forms, JavaScript and embedded files
do nothing. Fonts/cMaps are bundled with an in-memory factory; JavaScript
image decoders are same-origin files. `useWasm: false`; CSP permits neither
`unsafe-eval` nor `wasm-unsafe-eval`. Measure `disableFontFace: true` and
`isImageDecoderSupported: false` before deciding their cost.

Embedded fonts and JPEG images can still reach native decoders. Huge pages or
compressed data can exhaust the content process. A compromised reader content
process holds only the reader's three calls. On Linux, Ibis enables WebKitGTK
sandboxing before the first webview; the reader ships there only with it. After
a reader crash, that content hash stays closed for the run.

## Bounds and page labels

At most 10,000 pages; first page within 15 s, otherwise Try Again and Open in
Zotero. Images are limited to 100 megapixels plus a decoded-byte budget; lost
images produce a visible warning. Canvases are capped at 16 megapixels with
CSS enlargement beyond that. Virtualization draws the visible pages and one
either side. Reader code loads lazily; a 300-page invented PDF must release
resources to the hidden 200 MiB app budget within 2 s of closing.

Physical PDF indices, printed labels and annotation labels are distinct. The
engine decides labels and the reader displays them. A citable label comes from
`/PageLabels`, an annotation or a confirmed offset, never the physical index.
After NFC it has at most 16 letters, digits, `-` or `.`, with no control/format
characters. Other labels are shown as unknown. Disagreeing or repeated labels
ask; labels outside the reference's page range need confirmation.

A lone Arabic page number at the start or end of extracted page text must
match its label; Roman numbers are checked for Roman-style labels. Any
contradiction invalidates the document's citation labels, including when only
labels were requested. A page without a printed number gives no evidence.

## Notes, qualification and licences

Notes are parsed by html5ever into a typed tree, bounded to 1 MiB and 32
levels: paragraphs, headings, lists, quotations, emphasis, strong,
sub/superscript, code and text. Links lose targets; images, scripts, styles,
frames, forms and all URLs are dropped. React renders nodes, never HTML.
Comments and PDF text are plain text.

Before shipping, qualify WKWebView, WebView2 and WebKitGTK, worker CSP, worker
termination, hardening options and the Safari 18+ runtime requirement.
Unsupported engines show why the reader is unavailable. The canvas page source
proves layout only.

PDF.js and bundled parts need explicit entries in `NOTICE.md`; Cargo's licence
check cannot see JavaScript. Keep Apache-2.0, BSD, MIT and CC0 notices. Do not
bundle Liberation Sans (GPL-2.0 with a font exception) without the maintainer's
licence decision; use system sans-serif for unembedded Helvetica.
