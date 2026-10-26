# Shared CSL formatting

`Formatter::format(&snapshot, &request)` returns the same canonical
`CitationResult` for every client. `submit(Arc<Snapshot>, request, priority)`
returns a cancellable task; dropping a task cancels it. Callers use the result's
`copy_content(request.items.len())` gate before copying. Diagnostics are fixed
codes, with no reference text or JavaScript exception messages.

Canonical representations stay byte-identical to the processor's output.
Validation accepts only known script-glyph presentation equivalents and
the pinned HTML whitespace-pair conversion, with exact character and space
counts. Bibliography line folding is checked against processor text captured
before folding; the extra evidence never changes canonical output bytes.
Contradictory, multi-item or unavailable author-only intent is refused; processor
empty-form errors throw instead of becoming copyable sentinel text. NUL and unsupported
controls are rejected in inputs and outputs; ordinary text whitespace and
Unicode paragraph separators remain valid. The shared copy gate independently
rejects embedded NUL in text, HTML or RTF.

Two worker threads own separate QuickJS runtimes. Interactive work takes
priority; a running background operation is interrupted and restarted after
interactive work finishes, keeping its original deadline. Queued cancellation
releases its slot immediately, and queued deadlines expire even while
interactive work continues. Every interrupted or failing runtime is discarded
before the next operation.

Defaults are 8 interactive and 2 background slots, counting running work;
5-second interactive and 30-second background deadlines; 128 MiB of JavaScript
memory per runtime; a 512 KiB JavaScript stack on a 4 MiB worker stack; 4 MiB
input/output boundaries and at most 1,000 requested items. Processor caches hold
at most 8 style configurations per runtime; ICU caches hold at most 16
collators. Selected data is size-checked before allocating a JSON copy. Runtime
initialization and regular-expression execution share the interrupt boundary.

The five current styles and English locale are embedded from the existing
[pinned vendor assets](../../apps/desktop/vendor/csl/README.md). Their manifest,
licences and notices stay authoritative. There is no runtime file or network
loader. ICU4X handles collation and locale-sensitive casing; QuickJS alone
ignores locale parameters for those operations.

Ibis intentionally differs from document-aware Zotero rendering: it disables
`disambiguate-add-year-suffix` in the bundled style data before processor
construction and reports `YearSuffixWithheld`; IEEE produces an unnumbered
reference entry with `NeedsDocumentContext` and `ReferenceEntry`. These changes
never mutate the vendor files. Node comparisons use the same Ibis-safe style
configuration. A citation form the fallback cannot represent is refused.

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
RTF byte for byte, including multilingual synthetic records. Separate fixtures
exercise Turkish and Lithuanian casing. The normalized synthetic Preview Test
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
