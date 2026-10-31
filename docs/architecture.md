# Architecture

This is the target architecture for the rebuild agreed on 2026-10-26 ([decision 0028](decisions.md#0028-engine-first-replaces-0003-and-0005)). Code on `main` still follows the older design in places; anything rebuilt follows this document. Build order is in [ROADMAP.md](../ROADMAP.md).

```
                ┌─ ibis (command line): every capability, for people, scripts and AI agents
                │    --json on every command · ibis serve --stdio (JSON-RPC) · ibis mcp
 ENGINE ────────┼─ Desktop app: Tauri 2 shell + React/React Aria picker; links the engine in-process; opens ibis:// links
 (Rust crates)  ├─ Editor endpoint compatible with Better BibTeX Cite as you Write
                └─ Later: language server, launcher extensions
   │
   ├─ ibis-core     pure rules: model, citation request/result, key syntaxes, rich text, locators, search
   ├─ ibis-sources  Zotero (local API, private copy of the database), BibTeX/BibLaTeX, CSL-JSON, snapshots
   ├─ ibis-csl      citeproc-js in embedded QuickJS + ICU4X collation; official CSL styles and locales
   ├─ ibis-engine   typed methods: search, cite, show, styles, check, export…; JSON envelope, errors
   └─ ibis-cli      argument parsing and terminal output over ibis-engine
```

## Principles

- **The engine is the product; front ends are clients.** Every capability lives in the Rust engine as typed methods. The command line exposes all of it; the app, `ibis serve` and `ibis mcp` each expose an explicit subset suited to them (the picker does not administer the engine; headless clients do not open windows). A test fails when two front ends give different results for the same request. No front end formats, resolves or decides anything itself.
- **One citation request, one result.** Preview, copy, paste, export, the command line and the editor endpoint all call `cite(snapshot, request) -> result`. What you see is exactly what is pasted.
- **Only what Ibis can get right.** A field a format cannot carry is reported as a typed loss; a citation form it cannot carry is refused. Where correctness needs the whole document (numbering, ibid., 2020a/b), Ibis gives what is correct on its own and says why.
- **Read-only sources.** Never write to Zotero, `.bib` files or any library. Ibis keeps only the normalised data it uses; the source stays the original.
- **Stable identity.** An item is `library + native key`, never a citation key or a file path.
- **Never fail silently or guess data.** Skipped data becomes a warning; a failed paste leaves the citation copied with a reason; keys are never invented.
- **Offline by default.** No telemetry. Network access needs a recorded decision and is opt-in (DOI lookup is planned this way for 0.6).
- **Behaviour as data.** Presets, app rules and templates are validated data; nothing a user configures executes code.
- **Escape every output** for its target: LaTeX, Pandoc, Typst, org, HTML, RTF.
- **Keep private data private.** Paths never reach the webview; fixtures are synthetic; diagnostics use fixed codes, never raw text or payloads.
- **Local endpoints** bind to `127.0.0.1`, require a token and reject browser `Origin` headers.
- **Links** (`ibis://`) are untrusted input: they only fill in the picker and never copy, paste, change settings or read files ([decision 0030](decisions.md#0030-links-and-launchers)).
- **Speed is a feature, shown by numbers.** Every speed or memory claim has a measurement that runs. Optimise the hot path the measurement finds, not a guessed one. Limits block at the size people have and stretch beyond it with the same numbers, so a miss teaches without blocking (decision 0032).
- **Opinionated defaults, sharp knives on request.** The first run works with no setup and good choices already made. Anyone can change any choice, but nothing asks a person to configure what a good default can decide.
- **Fewer moving parts.** Prefer one process, standard formats and code Ibis already ships over new services, dependencies and custom infrastructure. When a simpler option makes code unnecessary, delete the code.
- **Long-lived foundations.** Standards and widely used open source first (CSL 1.0.2, CSL-JSON, BibTeX, SQLite, Zotero's APIs, PDF.js). Each dependency that could fade sits behind one Ibis module so it can be replaced: Tauri, React Aria, PDF.js, citeproc-js, the BibTeX parser.

## Data model (ibis-core)

- **Objects:** `Record` (a citable work: CSL-normalised fields plus citation key with aliases), `Attachment`, `Note`, `Annotation`, `Collection`, `Tag`, `SavedSearch` (name and conditions only). Each is addressed by `ItemRef { library, key }`; children point to their parent.
- **Availability** of an attachment is computed by Ibis the same way in every mode: `OnDisk`, `CloudPlaceholder`, `NotDownloaded`, `Online`, `BrokenLink`, `Unreadable`, `Unknown`. Standalone notes and files are shown without inventing a parent.
- **Library IDs are opaque** (`file:<hash>`, `zotero:<library>`). File locations stay in the engine and never serialise.
- **Snapshots are immutable.** A refresh builds a new snapshot and reuses unchanged libraries. Each object has two fingerprints: `cite` (what a citation depends on) and `full` (everything shown). A picker session keeps the snapshot it started from. At copy time, in one step against the newest snapshot, each pick is checked by `ItemRef` and `cite` fingerprint and, for key outputs, its key is checked to still name exactly that one item across all loaded libraries; a pick that was deleted, changed what it cites or lost a required key to another item is refused with the reason. Formatted citations do not require a citation key. The picker shows the snapshot's age when it is old and says when a refresh failed.
- **Source identity lifecycle:** a file library's ID follows its canonical path (a moved file is a new library; an edited file keeps its ID), and each chosen Zotero library (the personal library or a group, by Zotero's public group ID) keeps one ID across live and closed reads. A request names the libraries to read; a new choice replaces the old one in one publication once all its libraries are read, a missing group fails the read instead of falling back to another library, and Better BibTeX's local library IDs for groups are matched by unique group name (an ambiguous name means keys unavailable for that group). Default-directory discovery supplies only the closed fallback; API metadata does not prove a physical data directory. A single private source generation and ready flag retire prepared Zotero citations during an uncertain full read or detected switch; only a coherent publication makes the source ready again. The generation is captured with the render snapshot and checked again under the native copy barrier, alongside citation fingerprints and the prepared content hash.

The initial shared vocabulary is in `ibis_core::contract`; its generated schema is
`fixtures/engine-contract.schema.json`. Source adapters, formatters and front ends
adopt it in the ordered rebuild issues. Snapshots share their objects, so
combining libraries never copies or re-fingerprints them. Non-item objects use
source-namespaced native keys so tags and collections cannot collide with item
keys. Rich text is a fixed-depth tree of styled text runs; strings remain
untrusted data and are never injected as markup.

`Record.identifiers` carries canonical source-neutral search identifiers, including
arXiv eprints that have no printed CSL field. It is omitted when empty and affects
only the full fingerprint; changing search metadata does not invalidate a citation.
`identifier::of_record` combines these identifiers with recognised CSL fields.
Explicit identifier values pass through the same canonical parsers as pasted
queries; malformed or oversized values are ignored before arXiv/DOI equivalence.

## Sources (ibis-sources)

- **Capabilities are a fixed list per source kind**, not a plugin system. Zotero open and Zotero closed declare the same list, and a test enforces it; anything only one path can provide is not offered.
- **Zotero:** Ibis maps Zotero's raw item data to CSL itself, from a pinned copy of `zotero-schema`, instead of reading Zotero's `csljson` export (it changes between releases, plugins alter it, and it is the slowest part of a scan). Both access paths fill the same neutral item.
  - **Running:** the local API (needs Zotero's "Allow other applications"). Zotero 10 reconciles its complete item-version map and fetches changed raw records; missing IDs remove deletions. Zotero 7–9 conservatively reread all records. Reload, a changed API instance or a backwards library counter rereads everything.
  - **Closed:** only when no Zotero process holds the data directory, Ibis copies `zotero.sqlite` and its WAL or journal into a private directory (owner-only permissions, excluded from backups, size-checked against free space), checks that the source did not change during the copy (size, modification time and SQLite header before and after), runs SQLite's quick check on the copy, reads it and deletes it. A changed or failing copy is retried up to three times, then refused with a reason. Leftover copies are deleted at start. The live database is never opened. Versions are told apart by schema; a database newer than Ibis knows is refused with a message. Copying while Zotero runs (for a faster start) stays off until a test with concurrent writes, checkpoints and journal recovery shows it safe.
  - **Citation keys** are read from Zotero's `citationKey` field (Zotero 8+), a `Citation Key:` line in Extra, and Better BibTeX's key store (read as a copy, through its own versioned adapter). Each key keeps where it came from. When they agree, that is the key; when they disagree, the item shows the conflict and key outputs for it are refused until it is fixed in Zotero, because Ibis cannot know which key the document's bibliography uses. Aliases come from Extra. A key that matches zero or several items is refused for that item. An item's identity is always its `ItemRef`, never its key.
  - Zotero 10's local write API exists; Ibis stays read-only until an opt-in decision says otherwise.
- **BibTeX/BibLaTeX:** each entry is parsed independently, so a broken entry produces a warning with its line while the rest loads; work is bounded so hostile files cannot hang the engine. Long-lived clients watch files and only replace them after two identical reads. A watch failure keeps the successfully read snapshot usable and reports the fixed `live-refresh-off` warning; manual Reload remains available. One-shot CLI calls install no watches.
- **CSL-JSON:** `citation-key`, then `id`, is the key. An `id` only proves a match against a bibliography made from that same file, which the item's details say.

### Saved source generations (#1298, first storage slice #1468)

Provisional (2026-10-31, maintainer): this flat-file design is being compared with
no cache and with SQLite as a private record/generation store before more of it is
built (#1534); bundled SQLite
already ships for reading Zotero, so "no new dependency" is no reason to prefer either.
Same records, scorer and public behaviour; the result is recorded in decisions.

Adopt #1298's revised immutable flat-file design. One `Arc<PreparedLibrary>`
owns one source generation: opaque library identity, source observation, content
fingerprints, compact search rows, key/alias ownership and checked object offsets.
Readers retain that generation; an index never reads objects from another one.
Only configured sources are reopened. Unknown store formats are cache misses;
before 1.0 they are rebuilt without migrations. Use bounded ordinary file I/O,
with no new dependency, unsafe memory mapping, SQLite or hidden polling.

Search returns a typed `SearchSummary`: item identity, cite/full fingerprints,
title, creator/date/container display fields and bounded availability metadata.
It is not a truncated `Record` carrying the complete object's fingerprints.
The same engine contract serves the picker, CLI and protocol clients. Search
uses prepared contiguous text and offset tables; abstracts remain a separate
last-pass section. Global search passes, identifier fallback after scope,
deterministic ordering and the limit-prefix property stay exact across sources.

`show`, citation and export materialize only their requested canonical objects.
A citation also loads its annotation/attachment/parent closure; every CSL field
may affect a style, so selected citation data is never reduced to display fields.
Prepared citations retain that small object set and generation references.
Replace every full-library owner: the search index, engine registry slots,
watcher model, `ZoteroSource.current`, live-reader payloads and formatter caches.
Keeping any full `Snapshot` behind the compact index defeats the memory goal.

Import and refresh stream each validated object through canonical fingerprinting,
search preparation and the generation writer, then release its CSL tree. Retain
only compact identity, ownership and ordering metadata; preserve existing parser,
value, warning, work-budget and fingerprint semantics. The initial storage slice
must support this sink without requiring a full-library JSON snapshot first.

The cache directory is owner-only, verified for ownership, ACLs and links before
payloads are written and on reopen, and excluded from backups where supported.
Reuse the closed-reader private-storage contracts. An exclusive lease permits
one writer/cleaner; a second process operates uncached. Create a unique generation,
flush its complete contents, atomically replace and flush the manifest, then
publish. Failed I/O retains the accepted generation; no reader observes a partial
write. Clean only owned files after their generation readers release them.
Writers retain weak session authority and a per-source operation identity; check
that identity under the reservation/write lock before charging bytes or writing.
Reserve the aggregate 1 GiB and 512 directory-entry budgets before payload,
index and manifest writes or temporary-file creation, including publication's
scratch slot. Cancellation, failure and drop release the operation's stage and
reservation. Under the exclusive lease, reclaim verified abandoned stages while
preserving live stages and generation readers. Reclaim before rejecting an
oversized store, inspecting at most 1,025 entries per pass; incomplete traversal
or an unknown manifest forbids generation deletion. Storage failure remains a
cache miss and never grants source readiness.

Treat stored bytes as untrusted: bound file/section/object sizes and counts,
validate UTF-8, offsets, ordering and identities before indexing, and checksum
index sections. On materialization, validate canonical objects and recompute their
fingerprints; a stored fingerprint alone is never trusted. Source paths and
attachment locations remain engine-private, absent from summaries and diagnostics.

A file observation binds its canonical identity and strong content digest;
size/mtime are hints, and reload keeps two identical observations. A cached file
becomes citable only after a fresh parse of current source bytes; stored
observations, checksums and fingerprints grant no source or citation readiness.
An accepted, freshly parsed file snapshot remains usable if watching fails. Cached Zotero is searchable but unready for citation
until a coherent read. Preserve Zotero 10's server identity, monotonic counter and
complete item-version inventory/deletion reconciliation; older releases, Reload
and discontinuity still reread fully. Better BibTeX keys refresh independently
because item versions do not cover their changes. Do not invent a `since` path.

The desktop saves only warning-free, record-only `.bib` imports. Bounded objects reopened through
one retained `PreparedLibrary::get` may serve search immediately; stored observations, checksums
and fingerprints never grant readiness. Every reopen silently parses current source bytes, then
accepts the verified objects and diagnostics through the normal path, replacing any differences.
Cached search publishes `Reloading`, with no new `checked_at` until verification succeeds;
failed verification retains searchable references and diagnostics without claiming a fresh check.
The existing source-generation gate, shared with Zotero, refuses citation and copy until then;
the desktop uses its existing library-loading message. Cached bytes never seed a watcher that could skip that parse.
Attachments are not persisted. Full objects remain resident; #1251 and #1298 stay open.

Fresh verification captures the previous immutable snapshot and reuses a record allocation only
after parsing and comparing its complete canonical content, including identifiers. Tolerant
verification keeps its warnings and work limits; explicit Reload stays strict. An unchanged
100k cache and its verified snapshot share approximately 100k canonical payloads instead of
retaining 200k. This targets verification peak memory; it leaves 100k full objects resident and
does not establish the mostly-on-disk goal or the <400 MB budget.

One process-lived `FileCacheSession` owns the lease. Remembered `.bib` settings keep the canonical
path captured by preparation. The session records that identity before hydration, even when the
file has moved or disappeared, and retains a retargeted path's old and current identities.
Startup normalizes only the matching saved choice; refresh never saves a choice.
Failed choice saves retire cached bytes and disable storage for the session, while source verification continues.
Removal retires even an unfinished startup load's manifest and
allows bounded cleanup after readers release its payload. Selection and close invalidate work;
final acceptance serializes manifest and app publication. Parsing, indexing and watcher setup
remain in the background. Samples preserve the remembered cache; explicit Reload keeps two identical observations.

Only native measurement qualifies time to first search and added import/refresh cost: five fresh
process runs each at 10k/100k, counterbalanced against a valid baseline, retaining every result.
Separate cached search from verified citation readiness and rebuild startup; report process-tree
peak memory and refresh commit delay. Existing budgets hold.

For an engine-only comparison, build baseline/candidate release `file_cache` test executables
with the same `cached_verification_measurement` test, then run
`python3 crates/ibis-engine/tests/refresh_budget.py --cached BASELINE CANDIDATE --output target/cache-verification.json`.
The runner seeds outside measurement, predeclares five counterbalanced fresh-process runs per
build at 10k/100k and a baseline max/min ratio of 1.25 for each metric, and retains every result.
It rejects unexplained baseline excursions and reports cached first-search time, verified-ready
time, sampled process-tree peak and settled MiB separately from import/rebuild. Only native
integrated measurement proves actual resident savings, allocator release, webview overhead and
instant visible results; object-sharing tests and this engine runner cannot qualify those.

Background work owns the captured source selection and operation generation.
Delayed A after B or close/reopen may update neither manifest nor engine state.
Equal content still updates age, warnings, errors and readiness. Copy retains the
existing final barrier: current source readiness/generation, selected fingerprints,
annotation relationships, exact global key ownership and preview hash must agree.

Scenarios precede code: reopen/search/show/cite parity, corrupt-cache refusal,
failed writes, lease contention, stale completion and close/reopen. Qualify the
integrated app with at least five fresh-process runs at 10k and 50k, preserving
every result and reporting startup and process-tree memory medians, against the
limits in decision 0032. Report cache-hit startup separately from first
import/rebuild; neither storage tests nor cached results qualify the other.
#1534 decides whether compact generations and lazy objects (#1298) are still needed.

## Citations (ibis-core, ibis-csl)

- **Request:** ordered items (the same reference may appear twice); a citation form (normal, narrative, author only, year only, full, bibliography only) and "keep my order"; per item a typed locator (one of the 29 CSL 1.0.2 locator terms), rich-text prefix and suffix (italic, bold, sub, superscript), author suppression, and the annotation it came from if any; an output spec or named preset; an input language for parsing typed locators and an output locale.
- **Result:** text, HTML, RTF and a display tree; one outcome per item; typed losses; notices such as "needs the whole document"; a content hash that copy and paste must match.
- **Key syntaxes** are pure Rust, each with a capability table (which fields and forms it carries, its escaping, a fixture checked against its own tool): Pandoc, BibLaTeX (any command, multicite), natbib, LaTeX `\cite`, Typst, org-cite, MyST, MultiMarkdown, AsciiDoc, Jupyter, Jekyll Scholar, Scannable Cite, bare keys, CSL-JSON, Better BibTeX's CAYW JSON.
- **Formatted output:** Zotero's citeproc-js runs in embedded QuickJS (`rquickjs`) with collation from ICU4X, so the command line and the app give byte-identical output on every system. One formatter runtime runs on its own thread with a bounded FIFO queue, queued cancellation and running cancellation through QuickJS's interrupt handler. Memory and time limits bound each request, including its queue wait; the runtime is restarted after an interruption. Any official style and locale. Before formatting, Ibis applies Zotero's own data conversions (Extra fields, creators, dates, URL stripping for printed articles with pages); differential fixtures compare the result with pinned Zotero output. The formatter and AI agents then get only `ibis_core::citable` fields: note field overrides become fields, and notes, annotations and private links are dropped.
- **What a citation cannot know:** selected items (or even the whole library) cannot establish a document's numbering, year suffixes or ibid. Numeric styles give the reference entry with a notice (today's fallback), note styles give the full first note, and 2020a/2020b suffixes are withheld with a notice. Byte-identical output across front ends is necessary, not sufficient: correctness is checked against Zotero.

A result may report partial outcomes for inspection, but clipboard clients must
use `copy_content(expected_items)` to require all requested items, no refusals and
a matching content hash. An annotation pick carries its own cite fingerprint. Native clients obtain an opaque `PreparedCitation` from the engine. `commit_citation` checks its engine, preview hash, current picked records, annotation parent/file/page and any key bindings before invoking a native clipboard callback under the final publication barrier. Formatting and validation happen outside that barrier. The app holds its source/intent generation guard first, then enters the engine barrier; its callback only writes the supplied canonical content and never reenters the engine. The serialized method surface has no clipboard operation, and publication notifications use channels rather than synchronous app callbacks.
Omitted input language means `en-US`; omitted output locale means the style's
default, never the operating system's locale.

Today's key outputs use `ibis_core::syntax::cite(&Snapshot, &CitationRequest)`.
Its capability table drives supported forms and typed field losses; explicit
unsupported forms are refused. Source order and repeated selections are retained.
Key output writes instructions for the destination processor, whose style can
subsequently sort or format the citation. The CLI and picker prepare citations
through the shared engine; the picker copies only its prepared result. Core no
longer carries the pre-engine library model, file loaders or citation renderer.
Shared names, dates, settings choices, discovery and transport rules remain
where clients still use them.

The picker's small per-reference editor (#1366) sends its captured page and
Normal, Narrative or Omit author intent through the native preview request.
The native adapter uses `ibis_core::locator::page_value` only for the separate
`page_input` field containing raw Page editor text, removing an optional
page label once before constructing the request. A captured canonical locator
is passed through unchanged, including quoted values such as `S.12` or
`pages of notes`; form-only edits never turn it back into raw input. Sending
both a captured locator and raw Page input is refused. Recognised labels with missing or
invalid values are refused before preparation; they never fall back to literal
text. The shared numeric grammar accepts Unicode numeric characters, counts
characters for numeric bounds and keeps four-digit years ambiguous in unlabelled
searches. Its input length limit still bounds Page validation. The editor keeps
the raw input. Narrative maps to an item form; Omit author maps to author
suppression. A single BibLaTeX narrative pick selects `textcite` from the output
family. Other form support and field losses come from the engine; unsupported
mixed narrative groups are refused, never split into invented clusters. Indexed loss messages
identify the reference and field. Page/form edits invalidate pending copy and
preview identity, then prepare a fresh canonical receipt; copy still consumes
only that receipt. Hiding citation text retains formatting refusals and their
actionable explanations. This adds no polling or formatter runtime and retains
only one page string and form per selected reference.
Preview identity also distinguishes raw editor text from a canonical locator,
even when the strings are equal, so their receipts cannot be reused across edits.

Picker search asks the engine to interpret trailing locators in the same search
and snapshot as its result rows; pasted-list precedence stays in the engine.
The native boundary reads typed item identities and fingerprints, then constructs
the canonical citation request directly. Its locator wire names adapt the shared
29-term vocabulary without a second parser or JSON reconstruction.

Literal affixes escape default consumer smart punctuation as well as markup.
Pandoc narrative, plain LaTeX and Typst place prefixes outside the citation and
report a typed prefix/flattened loss, since a footnote-style citation cannot
carry that prefix in the same position. Capability tables list forms restricted to one item;
multi-item Pandoc narrative requests are refused until mixed clusters are built
in 0.4, rather than silently becoming several citation clusters. Pandoc affixes
isolate superscripts from citation brackets and use explicit locator boundaries,
so a literal suffix cannot become a page number. Locators containing braces,
brackets or backslashes alongside a suffix are refused when Pandoc cannot
preserve that boundary. Empty affixes add no punctuation.
BibLaTeX page locators followed by literal suffixes carry an explicit localized
page label; LaTeX locator quotes remain literal while range dashes stay intentional.

## Engine interface (ibis-engine, ibis-cli)

The rollout starts with the file-based engine and CLI, then ports today's picker
onto that surface. Zotero mapping and the closed/live readers proceed in parallel
on raw item data; #1197 integrates both providers, and #1200 gates full parity.
Advanced mixed citation editing remains in 0.4; unsupported intents are refused
explicitly rather than approximated. The 0.2 release still requires all sources and the picker. Engine `cite` routes
`OutputSpec::Csl` to the shared bounded formatter on the same immutable snapshot;
`styles` lists the five offline styles. Human CLI citation/bibliography options,
canonical JSON requests and stdio use those same results. RPC cancellation reaches
the formatter task, and exhaustion remains a typed resource-limit error.

- **Typed engine methods** are the source of truth for the command line, `ibis serve --stdio` (JSON-RPC 2.0, as language servers use), `ibis mcp` for AI agents, and the app. Each front end lists which methods it exposes; the app keeps per-window command permissions (decision 0024), so a generic dispatcher never widens what a window may call.
- **Every `--json` output is a versioned envelope** `{ "ibis": 1, "ok": …, "data" | "error": … }` with stable error codes, including argument errors. `data` carries partial results with per-item outcomes, losses and refusals; an empty search is `ok` with no results. Only the envelope goes to stdout; logs go to stderr.
- **Exit codes** describe how a command line run ended: 0 ok (including empty results), 1 done with findings (refusals, losses, source warnings or refresh failures), 2 usage, 3 bad input, 4 a named item or library not found, 5 source unavailable or resource limit exceeded, 6 newer format than Ibis knows, 70 internal error, 130 cancelled.
- **`ibis serve --stdio`** uses JSON-RPC 2.0 with LSP-style `Content-Length` framing, library-change notifications, `$/cancelRequest`, a message size limit and nothing else on stdout. Its errors are JSON-RPC errors whose `data` carries the Ibis error code; exit codes do not apply per request. `ibis mcp` keeps MCP's own envelope over the same methods.
- **The command line ships on its own:** every nightly and release publishes `ibis` for each system as an archive (`ibis-<system>.tar.gz`, `ibis-Windows.zip`) with the licence and third-party notices, checksums and the same build provenance as the app, and the release workflow runs each archive on its system before publishing (the Intel macOS archive is cross-built and not run). The app does not install it on PATH.
- **`ibis schema`** prints the JSON Schema of every method (generated with `schemars`); TypeScript types for the app are generated from it. `ibis call <method>` reaches any method from a shell.
- **Bounds and invalidation:** request JSON is at most 1 MiB and request IDs at most 1,024 UTF-8 bytes. Results share a 16 MiB response budget with 8 KiB reserved for ID/envelope overhead; oversized results return `resource-limit` (exit 5). Requests execute in order on one worker behind an eight-request queue; a long request delays later requests on the same connection. Running and queued requests can be cancelled. Array frames receive a single Invalid Request error (`-32600`, `id: null`). Library notifications are bounded wake-ups; subscribers read current library state after a wake. Refresh failures retain the prior snapshot and report its last successful update time plus a fixed error code.
- **Golden cases** (a new `cases` folder in `fixtures`: library, request, exact expected result) are replayed through every front end that exposes the method and must match byte for byte.

### Large-library measurement (#1251)

The release limits are in decision 0032 (2026-10-31): they block at 10,000
generated realistic records and a 50,000-record check reports on nightly. 100k is
no longer a target. Pull request CI fails on search p95 > 20 ms and picker assets
> 400 KiB gzipped. The rest of this section records the earlier 100k checks and
measurements, which still run until #1536 moves them to the new sizes.

The explicitly invoked `ibis-engine` test `refresh_budget` generates its own 21.05 MB
BibTeX file (two authors, journal, date, pages, DOI, volume and issue per entry).
The `open_refresh_budget` case opens the file, changes one record, refreshes
while committing a prepared citation every millisecond, and measures 250 searches (50 for each of five
queries, including first calls). Each query's p95 must meet 20 ms independently;
cheap queries cannot hide a slow case. The prepared citation
retains the old snapshot throughout refresh. It reports search p50/p95/p99.
The PR case, `search_budget`, loads the same library and checks search and
ranking without asserting open timing or performing refresh. Both stay ignored
in ordinary debug tests and run explicitly in release mode, alone. No real
library or large fixture is needed. Run from the repository root with a
worktree-local target directory:

```sh
CARGO_TARGET_DIR="$PWD/target" cargo test -p ibis-engine --locked --release --test refresh_budget -- search_budget --ignored --nocapture --test-threads=1
CARGO_TARGET_DIR="$PWD/target" cargo test -p ibis-engine --locked --release --test refresh_budget -- open_refresh_budget --ignored --nocapture --test-threads=1
CARGO_TARGET_DIR="$PWD/target" python3 crates/ibis-engine/tests/refresh_budget.py
```

The Python standard-library runner builds first, then measures the test process
alone with `getrusage` on macOS/Linux, excluding compiler memory. It also works
when a sandbox denies `ps` and macOS `time -l`'s `sysctl` call.

The first strict #1369 gate run on this arm64 Mac (2026-10-28, release) passed
open at 2,685 ms and refresh at 2,476 ms, but failed search p95 at 25.94 ms.
Temporary stage timers on its slow query (`synthetic signals 99999`) found
these median milliseconds; normalization (~0.006 ms), final result cloning
(~0.05 ms) and serialization (~0.1 ms) were not the bottleneck. Ranking already
selected top-k rather than sorting every match:

| Search stage | Before | After |
|---|---:|---:|
| Initial term matching | 5.69 | 5.89 |
| Exact-key scan | 7.29 | 1.05 |
| Typo/abstract/closest fallback work | 12.13 | 0.09 |

ASCII keys now compare lowercase bytes; other keys retain Unicode folding.
Each search reuses at most eight metadata term-score arrays and starts fallback
intersections from the rarest cached term. Scope, year and already-found filters
are still applied on every pass. The cache is discarded after the request and
adds at most 1.6 MB at 100k, plus the existing scratch scan. Empty abstracts skip
the redundant abstract pass. Ordering, scoring and closest-match semantics stay
unchanged: eleven cases compare ordered keys, scores and match flags against
digests captured from the original 100k scorer before these changes.

Three alternating fresh-process pairs, built beforehand without timers, gave
before p95 25.68–27.44 ms and after p95 7.49–7.56 ms. Each new run includes 250
samples (750 total); the worst per-query p95 was 7.85 ms, a 12.15 ms / 61% margin
below 20 ms. The slowest first call was 7.90 ms. Open was 2,657–2,776 ms and
refresh 2,407–2,559 ms. OS file caches persist across processes.
Before splitting enforcement, the combined gate also passed: search p95
7.16 ms (worst query p95 7.25 ms), open 2,758 ms and refresh 2,464 ms.

Enforcement is split by measured margin (maintainer, 2026-10-28):

| Budget | Enforced in | Reason |
|---|---|---|
| Search p95 ≤ 20 ms, each query | `ci.yml` → `test` (`Test (ubuntu-24.04)`), release `search_budget` | At least 61% measured margin |
| Picker JS/CSS ≤ 400 KiB gzipped | `ci.yml` → `changes` and `web`, both picker build paths | Deterministic asset bytes |
| 100k open and refresh < 3 s each | `preview.yml` → Linux `build`: `open_refresh_report` on branches and 0.2.x, `open_refresh_budget` on tags from 0.3 | Only 224 ms open margin; shared PR-runner noise can exceed it |

Preview's build covers nightly packages from `main`, tagged releases and manual
package builds; the scheduled update-test-only run retains its existing build
skip. Failing open/refresh blocks that build and its downstream publication.
Both engine cases use one test thread after compilation finishes; no retries,
discarded warmups or best-of selection hide failures. One instrumented run
immediately after compilation took 3,638 ms to open while search passed, which
is why open/refresh timing no longer rejects a PR. The limits are unchanged.
These local engine runs do not substitute for native release-machine measurements.

Measured on this arm64 Mac on 2026-10-28, release build; the baseline is
`28bfce26` (`origin/main` when work began), exported without git metadata and
built in a separate worktree-local target directory. Both builds use the same
measurement harness, input and copy cadence. Three alternating runs without
profiling instrumentation gave these ranges; RSS is the highest peak of the
three runs, rather than an average:

| Metric | Before | After |
|---|---:|---:|
| First open | 3,164–6,874 ms | 2,705–7,293 ms |
| One-record refresh | 3,521–7,101 ms | 2,511–5,343 ms |
| Commit during refresh, p99 range / maximum | 16–127 / 6,691 µs | 16–155 / 6,455 µs |
| Search p95 | 25–27 ms | 25–62 ms |
| Maximum process peak RSS | 1,140.8 MiB | 749.5 MiB |

All final runs were below the earlier 877 MiB main observation as well as their
fresh baseline (final RSS range 607.9–749.5 MiB; baseline 1,035.9–1,140.8 MiB).
The 400 MB memory budget remains unmet. Timing varied substantially with machine
load, so these runs do not establish a latency improvement or certify the 3 s
load/refresh budget. Typing and native summon still need the app probes.

Temporary timers on the same release measurement identified CSL conversion as
the largest source stage. The following are milliseconds for **open / refresh**,
before and after removing intermediate text-normalization allocations from the
resumed implementation. These two runs were consecutive, with both binaries
built beforehand. Refresh also includes the unchanged 250 ms settling wait
outside these stage totals:

| Stage | Before normalization change | After normalization change |
|---|---:|---:|
| Read (both observations on refresh) | 4.5 / 11.4 | 4.6 / 13.6 |
| Block scan | 45.6 / 44.8 | 43.8 / 43.2 |
| Raw entry parse, including duplicate-key checks | 170.3 / 175.6 | 169.9 / 167.3 |
| Macro/crossref resolution, including setup | 539.8 / 564.1 | 521.4 / 566.9 |
| Upstream BibTeX field parse | 377.1 / 372.8 | 380.1 / 381.6 |
| CSL conversion | 816.9 / 762.4 | 787.1 / 730.7 |
| Record deduplication/reuse | 4.2 / 148.9 | 4.0 / 159.1 |
| Snapshot validation, sorting and fingerprints | 411.5 / 82.1 | 416.4 / 81.5 |
| Text normalization (included in CSL conversion) | 112.2 / 110.2 | 72.7 / 72.7 |

The focused change removes a token vector and a joined string for every cleaned
field/name, copying ordinary words directly into the final string. It sizes that
string from normalized lengths so whitespace-heavy fields retain no discarded
whitespace. Unicode whitespace and literal-placeholder decoding stay unchanged.
The helper takes about 35% less time; total source/snapshot open stages fell
from 2,370 to 2,327 ms in this pair. This is an allocation fix in the largest
stage, with no parser rewrite or new cache. Profiled whole-process peak RSS was
748.2 / 751.2 MiB before/after; both remain below main.

Deduplication now compares normalized records against the prior validated model
before collecting a candidate: refresh fingerprints only its one changed record.
The extra comparisons avoid accumulating 99,999 duplicate CSL trees. The timers
were removed after measurement.

Main already shared fingerprinted objects after #1310. This change batches JSON
writes to SHA-256 without changing their bytes, hashes identical cite/full views
once, and computes aggregate fingerprints once per combined publication. A sole
library shares its entire object slice with the source. BibTeX refresh reuses old
records while parsing; other objects are coalesced by full fingerprint before a
watcher retains the next model. Its callback acknowledges the installed,
coalesced source snapshot; rejected publications leave the prior model intact.
Only explicitly seeded watchers keep a model. This prevents a watcher from
holding a second, pre-coalescing copy of the library. Index refresh reuses
prepared keys, identifiers and normalized text. Assembly and index preparation precede the
brief state swap, so they do not hold the citation commit's publication barrier.
Raw literals reserve one part instead of four. Header deduplication tables are
released before conversion, and CSL construction moves normalized strings into
JSON values instead of serializing and copying them again. Macro/crossref and
field-parser algorithms are unchanged. The remaining footprint includes the
normalized library, index, source text and raw-entry working set; reaching 400 MB
still needs further work.

## Desktop app

- **Shell:** Tauri 2 with the system web view; React with React Aria Components for every control (decision 0021); tokens in `tokens.css`; shared components in `src/ui/`. The app links the engine in-process and calls the same method functions; the web view never touches the clipboard, files or the network.
- **State:** the citation session uses one pure reducer read through `useSyncExternalStore` and a headless controller with session and operation identities; search/preview I/O, destination choices and preferences remain in their adapters; no state library.
- **Output per app:** the destination table classifies known browser and code-editor app IDs as needing an output question, without inferring document syntax. The native destination reply carries only this category alongside the existing app ID/name and outputs. The first copy with no remembered app output holds its citation in the picker and opens the existing chooser; a pick or dismissal resumes it once when its preview is ready. Search/citation changes, hide/reopen and menu commands cancel the held action. Picks use the existing bounded, atomic `apps.json`; no new file, polling or document inspection. Only macOS currently captures an app; other systems retain the global fallback. Classification adds one bounded table lookup per destination read and no hidden activity.
- **Keys:** one command registry (id, default chord per system, user override) drives the keys sheet, footer, menus and docs. Conflicts are checked against Ibis's own keys, a reserved list per system and a real registration attempt. Escape, Tab, Enter and arrows cannot be rebound.
- **Pasting into the previous app:** a pure decision in the engine and thin per-system code. macOS: the app in front is captured (bundle ID, process, name; never window titles) before the picker shows. A successful copy records the pasteboard change count, target, picker presentation and write time in a one-use proof. After the picker hides, one layout-aware ⌘V is posted only to that process. The final foreground, Secure Event Input, change-count and deadline checks run together with posting in one main-thread task. Secure Event Input is system-wide, may be enabled by another app, and does not identify the destination's field or detect every password control. An enabled flag copies with `secure-input`; unavailable checks or expired tasks copy with `paste-unavailable`. Both notices offer ⌘V; neither claims the destination is a password field (decisions 0009, 0029). A replacement clipboard or changed picker also only copies. Paste reads no AX attributes or clipboard contents and requires only PostEvent permission, independent of AX trust. Terminals, remote desktops and VMs are also refused. Windows: the window in front is captured the same way (executable file name, process, integrity level), the picker shows above always-on-top windows, and the keyboard is handed back to that window. A one-use delivery ticket freezes the target, HWND and picker generation before copying, then records the final clipboard sequence after Ibis's own write. One atomic Ctrl+V `SendInput` goes in only while that window and process are in front, no key is held and that sequence is unchanged; apps with more rights than Ibis and UI Automation password fields copy instead. Password checks have a 250 ms caller deadline, no queue and at most one outstanding query per target process (four total); a stuck provider blocks only its own process. Display names are read off the UI thread with a bounded wait and executable-name fallback. Partial `SendInput` releases every key it left down, retries failed releases and retains them for recovery before any later paste. Linux copies and says why. Every refusal leaves a copied outcome. macOS keys PostEvent permission to the signing identity, so builds need a stable signing certificate.
- **Reader (Later; 0.5 cites from annotations and existing PDFs first):** PDF.js (`pdfjs-dist`, legacy build) behind a small adapter. File bytes arrive over IPC from a command that checks the item and never exposes paths; PDF scripting and eval are off; a plain page-text view backs up accessibility. Ibis keeps the physical page index, the printed page label and an annotation's own page label apart. The printed label comes from the PDF's page labels, a Zotero annotation on that page, or an offset the user confirms; a confirmed offset covers a page range (front matter, missing pages and supplements break a single offset), is tied to the file's content hash and is dropped when the file changes. The record's page range is only a hint.
- **Accessibility:** combobox and listbox (not grid) for results, `TagGroup` for the shelf, one announcer, native announcements after the picker hides. Tested with axe in jsdom, Playwright WebKit snapshots, native accessibility trees and screen-reader runs per release.
- **Paste permission detour:** delivery returns its typed outcome to the picker.
  A `no-permission` result keeps the prepared shelf and output in memory rather
  than consuming them. Settings rechecks permission every second while its
  General page is visible and pasting is on, and immediately on return; older
  reads cannot replace newer ones. Only a refused-paste deep link resumes the
  picker, after another native permission and focus check and queued settings
  saves. Normal picker presentation captures the current destination and
  retires the old clipboard proof. No paste is attempted until a fresh Enter.
- **Language:** the engine returns message codes; the app words them through Fluent from 0.3, so German and one right-to-left language can follow in 0.4. Interface language, citation language and text direction are separate; mixed-direction names are isolated in the preview and on the clipboard.
- **Budgets (release builds, limits from 0.3):** the limits and sizes are in decision 0032; #1536 gives each its check and channel. No polling loops while hidden. Engine search and picker asset size fail pull request CI; open latency, idle CPU, wakeups and whole-app memory (web view processes included) are measured warm and cold, at least 30 samples after a 60 s settle, on the named release machines before each release. All use invented libraries, report p50/p95/p99 and compare against fixed limits rather than only against `main`. Superseded previews and searches are cancelled, not ignored.

The picker asset gate (`node scripts/picker-budget.mjs`) reads Vite's manifest
and sums gzip level-9 bytes for `index.html`'s JS, static imports and CSS,
deduplicated by file. Other windows and lazy imports are excluded. Missing
entries/assets fail the check. Both native and interface-only PR builds run it.
Library notification workers block on channels. EOF or replacing the desktop
library disconnects the subscription, waking the worker without a timer or
retaining the old engine indefinitely. Search cancellation reaches the index;
reserving a replacement preview cancels the prior engine/formatter token,
including a worker that has not started yet.
Clearing the selection or unmounting the preview aborts the same engine token.
Page/request numbers keep late aborts and late worker starts from cancelling a
replacement; cancellation messages are handled before taking the library lock.

### Native release measurements (#1369)

Before each release, use the exact release revision/build on each named macOS,
Windows and Linux release machine. Record its stable machine name and platform.
Use only generated libraries at 10,000 and 100,000 references. For each size,
collect at least 30 cold-picker and 30 warm-picker samples after settling hidden
for at least 60 s. Cold means the first shortcut presentation after a fresh
process has loaded its snapshot and settled; warm means a subsequent
hide/summon in that process. Disk/source startup time is the separate engine
open/refresh gate, not shortcut latency.

Measure shortcut-to-interactive and typing-to-results latency with native tracing,
not accessibility tree polling. While hidden, record CPU as percent of **one core**, wakeups per
second, and the entire process tree's resident/working-set bytes, including
webview helpers. Use platform profilers (Instruments on macOS, WPR/WPA on
Windows, perf on Linux); record idle CPU/wakeups over one-second intervals.
Shared pages may be counted twice when summing process RSS; never omit helpers.
Keep the raw traces locally and export only these fixed numeric fields:

```json
{
  "schema": 1,
  "revision": "0123456789abcdef0123456789abcdef01234567",
  "platform": "macOS",
  "machine": "synthetic-release-machine",
  "measurements": [{
    "mode": "cold", "references": 10000, "settle_seconds": 60,
    "samples": [{"open_ms": 85, "typing_ms": 30, "idle_cpu_percent": 0.1,
                 "wakeups_per_second": 0, "memory_bytes": 150000000}]
  }]
}
```

This abbreviated shape is not passing evidence: all four size/mode cases need
30 samples. `node scripts/release-budgets.mjs evidence.json RELEASE_SHA` checks
the exact revision, completeness, settling and fixed limits, and reports
p50/p95/p99. Open p95 must be ≤ 100 ms and typing p95 ≤ 50 ms;
every idle sample must be ≤ 0.2% CPU
and ≤ 1 wakeup/s. Memory must be ≤ 200 MiB at 10k and < 400 MB at 100k.
Run the validator for each platform and attach its successful output to the
release evidence. `scripts/smoke-native.py`'s debug launch/accessibility timings
are explicitly diagnostic-only and cannot satisfy this gate.

Actual release machine names, native trace collection and release-workflow
evidence attachment still require the release operator; no measurements or
machine certifications are supplied by the validator.

## What gets replaced

The pre-engine `Reference` model, library selection helpers, owned search index,
file loaders, watcher, citation renderer and Zotero CSL-export mapping and scan
have been deleted from `ibis-core`. BibTeX parsing and file watching live in
`ibis-sources`; Zotero reads use `ibis-zotero-local` and the canonical mapping in
`ibis-sources`. Search uses contract snapshots and citations use the shared engine.
Core retains the shared name/date helpers, settings, discovery, transport and
group-list rules still used by clients. The native shortcut, panel, clipboard and
updater modules, UI components, tokens and synthetic fixtures remain. Settings
migrations and identity carry-over code are deleted (decision 0027).
