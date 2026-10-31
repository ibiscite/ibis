# Shared CSL formatting

`Formatter::format(&snapshot, &request)` returns the same canonical
`CitationResult` for every client. `submit(Arc<Snapshot>, request)`
returns a cancellable task; dropping a task cancels it. Callers use the result's
`copy_content(request.items.len())` gate before copying. Diagnostics are fixed
codes, with no reference text or JavaScript exception messages.

Source conversion warnings stay on immutable records and participate in citation
fingerprints. Results carry fixed warning codes with request item indices only
for rendered items; warning text and source locations never reach JavaScript.
BibTeX titles retain stored case and protected `nocase` spans. APA uses citeproc's
constructor option `uppercase_subtitles`, as Zotero does, and returns the
`sentence-case-expected` advisory for English reference titles rather than
guessing which capitalized words are proper nouns. Other styles retain their
native title presentation. DOI wrappers (`doi:` and HTTP(S) DOI resolver URLs)
are removed at the shared citable-field boundary, preserving identifier bytes.

Canonical representations stay byte-identical to the processor's output.
Validation accepts only known script-glyph presentation equivalents and
the pinned HTML whitespace-pair conversion, with exact character and space
counts. Bibliography line folding is checked against processor text captured
before folding; the extra evidence never changes canonical output bytes.
Contradictory, multi-item or unavailable author-only intent is refused. Empty
members receive typed refusals before processor placeholders can become copyable.
Inline CSL string values fold runs of line breaks to one space and replace
unsupported control characters before formatting. Request controls and invalid
output controls are rejected. The shared copy gate independently
rejects embedded NUL in text, HTML or RTF. JavaScript stack and heap exhaustion,
including the processor's exact native-error forwarding forms, return the fixed
`ResourceLimit` code; arbitrary exception text never crosses the boundary.

Canonical rich title fields retain the source adapters' escaped text and allowed
markup. The bridge decodes entities once and normalizes decoded line breaks and
controls before processor casing and sorting. Escaped less-than signs stay inert
until each output format escapes its text. One unused private-use code point is
reserved against both raw and decoded input; processor tag removal cannot assemble
it from adjacent characters. Exhausting the available code points refuses output.
Plain fields, names and typed affixes do not undergo entity decoding. Ordinary
ALM, LRM and RLM direction marks retain their meaning, including signs in RTL text.
Embedding, override and isolate controls are removed from fields, locators and
affixes before rendering, including encoded controls; RTL letters and shaping
characters remain unchanged. A raw unsafe control inside tag-looking text refuses
the request, since removing it could create a formatting instruction.

Literal years keep their numeric sort order separately from their rendered text.
The shared year parser supplies private formatter sort data after source custom
fields are removed. Only the processor's date-sort call sees a scratch numeric
date; the registered literal remains unchanged. Explicit year letters sort in the
processor's `a` through `z`, then `aa` sequence, without generating new letters.
Curly-quoted raw dates become literals at the shared citable-field boundary;
ASCII-quoted raw dates retain the pinned parser's quote handling. Nonempty
structured date parts take precedence; otherwise a nonempty raw string replaces
ignored literal, season and circa values, including when raw is only whitespace.

One worker thread owns a QuickJS runtime and a bounded FIFO queue. Queued
cancellation releases its slot immediately; running work is cancelled through
QuickJS's interrupt handler. Deadlines include time in the queue. Every
interrupted or failing runtime is discarded before the next operation. Shutdown
cancels pending work and joins the worker; worker failure refuses further work.

Defaults are 8 slots, counting running work; a 30-second request deadline;
128 MiB of JavaScript memory; a 2 MiB JavaScript stack on an 8 MiB worker stack; 4 MiB
input/output boundaries and at most 1,000 requested items. The software-stack
maximum is also 2 MiB; the native stack is four times that cap. Windows measurements
found 1 MiB necessary for a synthetic APA citation; the default doubles that
measured minimum. Direct runtime tests use controlled worker-sized threads,
including hostile recursion. Each request registers its items once, then produces
text, HTML and RTF from that same processor state. The processor keeps one warm
style configuration per runtime and drops request items after the call; ICU
caches hold at most 16 collators. Selected data is size-checked
before allocating a JSON copy. Runtime initialization and regular-expression execution share the interrupt boundary.

The five current styles and English locale are embedded from this crate's
[pinned vendor assets](vendor/README.md). Their manifest,
licences and notices stay authoritative. There is no runtime file or network
loader. ICU4X handles collation and locale-sensitive casing; QuickJS alone
ignores locale parameters for those operations.

Ibis intentionally differs from document-aware Zotero rendering: it disables
`disambiguate-add-year-suffix` in the bundled style data before processor
construction and reports `YearSuffixWithheld`; styles declaring
`<info><category citation-format="numeric"/></info>` produce an unnumbered
reference entry with `NeedsDocumentContext` and `ReferenceEntry`. `label` does
not imply document numbering. The processor's XML tree removes bibliography
layout `citation-number` text nodes (any affixes) and `second-field-align`,
following macro calls through bibliography-only clones to preserve citation
rendering and sort keys. These changes
never mutate the vendor files. Node comparisons use the same Ibis-safe style
configuration. Numeric fallback refuses author-only and year-only forms.
Cached XML traits also drive the bridge. Year-only remains eligible when `issued`
is reachable from the citation layout, including conditional macro calls. Each
year-only item's emitted text must contain its issued year in the style's year
form, or an emitted blob identified by citeproc as the style's `no date` term
for an undated item (including its zero-year sentinel). Matching title or locator
text and another item's year cannot establish coverage. Macro names must use
matching XML encodings.
#1306 owns `YearSuffixWithheld`'s
remaining name check; its general rule is `disambiguate-add-year-suffix="true"`. A transparent
observer checks each final member's printed blob leaves in every representation,
ignoring layout affixes. Missing or empty coverage refuses the candidate cluster as `UnsupportedForm` and clears its output; no unchecked member
is exposed as rendered. This uses one pass over existing output under the same
heap/deadline limits, and never scans for processor marker strings.

The 0.2 adapter carries today's locators and author suppression. Prefixes and
suffixes, including plain and styled runs, are omitted with separate typed
`Prefix`/`Suffix` losses until the 0.4 citation builder supplies a correct rich
affix implementation. Bibliography-only output also reports locator or author
suppression loss and does not promise caller-controlled sorting. Non-English
metadata is supported; explicit output locales other than `en`/`en-US` are
refused until more locale assets are bundled. Raw Zotero-to-CSL conversion belongs
to the source adapter, not this crate.

# Evidence

`cargo test -p ibis-csl --locked` runs normal Rust tests and an offline Node
oracle (Node must be on PATH; `IBIS_NODE` can select its executable locally).
Ten five-style citation/bibliography scenarios compare text, HTML and complete
RTF byte for byte, including multilingual synthetic records. Nine year-only
scenarios also cover dated, undated and zero-year items in the supported styles.
The Node oracle restores the original processor function to prove that observing content leaves
does not change those bytes. Separate fixtures exercise Turkish and Lithuanian
casing. The normalized synthetic Preview Test
also matches the immutable expected `(Smith, 2019)` in
[Zotero's own test](https://github.com/zotero/zotero/blob/db41b99fddcc52d38897c59c161b6adaca036afb/test/tests/citeTest.js#L175-L203).
This is pinned expected-output evidence, not a claim that a running Zotero app
or every Zotero conversion was tested.

The measurement test runs in the existing three-platform Cargo CI checks. It
prints successful measurements directly to their logs: cold formatting, warm
one-item p95 (10 samples), and a 100-entry multilingual bibliography. Go/no-go
ceilings are 2 seconds, 250 milliseconds and 10 seconds respectively. They are
debug-harness measurements with optimized dependencies, not release benchmarks
or proof of the separate picker/search budgets. Output integration tests are
serialized so another formatter in that binary does not contaminate those
wall-clock samples. CI on the exact head supplies the Windows/Linux evidence;
a local macOS run alone does not establish cross-platform success.
