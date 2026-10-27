# Engine golden cases

These synthetic seeds define the shared contract for the engine rebuild
(#1192, ADR 0028). Each JSON file has `name`, `library` (an immutable `Snapshot`),
`request` (`CitationRequest`) and `expected` (`Envelope<CitationResult>`).
Everything in them is invented; opaque library IDs contain no file locations.

The contract test parses and round-trips every case, validates the snapshot
fingerprints and checks each successful result's content hash. It does not yet
render citations. #1193 replays key-output cases; #1198 checks formatted output
against pinned Zotero; #1199 replays the engine cases and #1200/#1201 compare
front ends exposing the same method byte for byte. Update a seed with evidence
if the formatter's canonical representation changes during the rebuild.

| Case | Intent |
| --- | --- |
| 01 | An empty selection is a request error, not a successful empty citation. |
| 02 | One Pandoc key. |
| 03 | The same work twice, in order, with independent locators. |
| 04 | A typed chapter locator. |
| 05 | A Unicode title and citation key. |
| 06 | Semantic italic and bold affixes. |
| 07 | A bare key loses its locator, reported by a typed loss. |
| 08 | IEEE falls back to its unnumbered reference entry and explains why. |
| 09 | Conflicting source keys refuse the item and prevent copying. |
| 10 | An APA bibliography entry, with italic title in the display tree. |
| 11 | Natbib with an explicit page locator and literal pre/postnotes. |

Case 11's `11-natbib.tex` consumer fixture uses Natbib's standard commands and
invented bibliography data. Compiling it twice with `pdflatex -no-shell-escape`
produces `(see Taylor, 2026, p. 12, note)` for the golden citation. It also checks
the prefix outside narrative and author-only citations: Natbib's native textual
prenote appears after the author, while `citeauthor` omits it entirely. The engine
therefore retains those prefixes outside the command and reports flattening.
The separate `11-natbib-numbers.tex` fixture demonstrates why author-only
locator/suffix requests are refused: in standard numerical mode, the citation
with postnotes and the citation without them both print only the author. The
failing-first Core and file-to-engine regressions prevent such a request from
reaching the copy gate.

The plain text of cases 08 and 10 was checked against the repository's bundled
citeproc-js and CSL styles with the case's synthetic CSL record and `en-US`:
trim the bibliography entry's terminal newline, and for IEEE remove its
context-dependent `[1]` label. This checks the seed text, not Zotero parity or
future embedded-runtime behavior. HTML and RTF remain absent in these seeds;
the formatting lane adds its exact canonical representations and differential
evidence. Rich affix syntax in case 06 specifies the new key-output contract,
not the old plain-string formatter's behavior.

A citation result's ordered outcomes refer to request indices. Refusals remain
inside a successful envelope for an otherwise valid request; the envelope
itself describes request-level failure. Copying always uses
`CitationResult::copy_content(request.items.len())`, which requires all
requested outcomes and the exact canonical content hash. Neither a partial
result nor an empty result may be copied.
