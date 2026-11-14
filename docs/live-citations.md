# From Ibis to live Zotero citations

Ibis writes text for a later conversion step. A pasted marker or Pandoc key is
not yet an editable Zotero citation. Keep the original document and convert a
copy. The following workflow follows the [ODF/DOCX Scan documentation](https://zotero-odf-scan.github.io/zotero-odf-scan/).

You need Zotero with the cited items available locally, the third-party ODF/DOCX
Scan add-on, and either LibreOffice with Zotero's LibreOffice plugin or Word
with Zotero's Word plugin. Zotero's word processor plugins are bundled with
Zotero; see its [installation instructions](https://www.zotero.org/support/word_processor_integration#plugin_installation).
The scan add-on is a separate component. Its pinned oracle version here is
[v3.2.2, commit 27a6ae72e6bb048fa808cd987f95327f67efd70f](https://github.com/Juris-M/zotero-odf-scan-plugin/commit/27a6ae72e6bb048fa808cd987f95327f67efd70f).

1. In Ibis, choose **Zotero RTF/ODF Scan** (`scannable-cite`) for references read from Zotero, and paste
   the markers into your document. They identify items in their original
   personal or group library; keep the five fields intact. The readable title
   is not the identity. A `.bib` or CSL-JSON file's claimed URL or item key cannot
   give it Zotero provenance in Ibis.
2. Alternatively use **Pandoc** keys, after checking each key is the exact
   Citation Key of its intended Zotero item. The pinned converter searches the
   personal library and takes the first result if multiple items match. Use
   unique keys; this check does not qualify group-library Pandoc lookup or
   duplicate-key resolution. Better BibTeX keys must agree with the field this
   converter searches; installing Better BibTeX alone does not establish that.
3. Save the copy as `.odt` for LibreOffice or `.docx` for Word. In Zotero, open
   **Tools → ODF Scan**, select the appropriate markers/Pandoc-to-citations
   direction and document type, choose separate input/output files, and process.
4. Open the converted file in its matching word processor. In LibreOffice, use
   Zotero's **Set Document Preferences**, choose a style, and format the
   citations. In Word, use Zotero's **Document Preferences** and **Refresh**.
   Add the bibliography with the plugin, then use **Add/Edit Citation** and
   open a cited item to check its identity. Save, close and reopen the copy to
   check that the citations remain editable. See the primary [Word](https://www.zotero.org/support/word_processor_plugin_usage)
   and [LibreOffice](https://www.zotero.org/support/libreoffice_writer_plugin_usage) plugin instructions.

For Google Docs, first complete conversion in Word or LibreOffice. You need
Zotero running and the [Zotero Connector](https://www.zotero.org/support/google_docs)
in the browser. On a new copy, open Zotero's Document Preferences in the source
word processor and choose **Switch to a Different Word Processor…**, then save.
Upload that transfer copy using File → Open inside Google Docs. If it opens in
Office editing mode, choose File → Save as Google Docs. Finally choose Zotero →
Refresh. Simply uploading the ordinary converted file can lose citation links.
Follow Zotero's [Word/LibreOffice-to-Google-Docs transfer instructions](https://www.zotero.org/support/kb/moving_documents_between_word_processors)
for the reverse journey too.

## Evidence and remaining native checks

Run from the repository root with Node on PATH and loopback listeners permitted:

```sh
RUSTC_WRAPPER= CARGO_TARGET_DIR="$PWD/target" cargo test -p ibis-engine --locked --test odf_scan -- --nocapture
```

This test invokes `scripts/odf-scan.mjs` directly with public-engine output.
It runs the pinned upstream ODF composition and Pandoc conversion methods,
then upstream URI lookup against invented records. [Oracle provenance and
licence notices](../fixtures/odf-scan/README.md) accompany the small excerpts.
There are no downloads, extra Node dependencies or personal Zotero profiles;
the existing Rust fixture uses an isolated loopback HTTP server. Both Rust tests
must pass, printing actual and expected identities and citation options for
every scenario. The group case also changes the marker to name the personal
library: it must resolve to the personal item and fail the group-identity
comparison. The affix case substitutes literal entity-encoded markup for italic
syntax and must fail the formatting comparison. A denied
loopback bind is an environmental failure; rerun outside that sandbox. A passing
file-only test or manually supplied markers alone do not complete this check.

The differential checks personal/group identity (including a shared item key),
adjacent repeated references and their order, page ranges, chapter and paragraph
locator text, author suppression, italic/bold affix markup, an exact personal
Pandoc key, refusal to substitute a similar key with an identical title, and
file-only identity refusal. The ODF converter keeps locator labels in the locator
text; these checks inspect that representation before Zotero renders it.

This is parser/conversion evidence with mocked Zotero APIs. It does not exercise
ZIP packaging, the DOCX converter, real Zotero search/database behaviour,
word-processor plugins, bibliography rendering, application save/reopen, or the
Google Docs transfer. It does not qualify Zotero 10: the shared fixture's version
header is synthetic. Actual Zotero/word-processor save-and-reopen remains
unqualified.

To qualify a native combination, repeat the scenarios in a disposable library
and document, verify item identity and citation options with Add/Edit Citation,
refresh, add a bibliography and save/reopen. For Google Docs also perform the
documented transfer and return. Record exact Zotero, scan add-on, word processor,
Connector/browser and OS versions with the resulting invented documents before
claiming that combination works.
