# Architecture

Ibis has a Rust engine, a command line and a Tauri desktop app. This page
describes this checkout. Plans live in [ROADMAP.md](../ROADMAP.md).

## Crates and clients

| Location | Responsibility |
| --- | --- |
| `crates/ibis-core` | Shared model, citation requests/results, key syntaxes, search and delivery rules. |
| `crates/ibis-zotero-local` | Zotero local API client. |
| `crates/ibis-sources` | Read-only file and Zotero adapters, discovery, database snapshots and file watches. |
| `crates/ibis-csl` | citeproc-js in QuickJS, ICU4X collation, bundled CSL styles and locales. |
| `crates/ibis-engine` | Typed methods, source lifecycle, prepared citations, settings owner, recents, JSON-RPC and MCP. |
| `crates/ibis-cli` | The `ibis` executable. Depends on core and engine. |
| `apps/desktop/src-tauri` | Native windows, clipboard, paste, shortcuts, settings dialogs and updates. Depends on core, engine and its command macro crate. |
| `apps/desktop/src` | React interface, shared controls in `ui/`, tokens in `tokens.css`. |

The desktop links the engine in process, with generated TypeScript bindings.
Contracts for the CLI, JSON-RPC and MCP are in [the CLI reference](cli.md); the
desktop also provides the [editor endpoint](parity.md#better-bibtex-cite-as-you-write).
Library results carry `source_health` from the state they read; later refresh
failures cannot change it.

## Data and identity

`ibis_core::contract` defines records, attachments, notes, annotations,
collections, tags and saved searches. A type existing in the contract does
not mean a source supplies it: Zotero readers currently load references,
not annotations or notes.

Objects use `ItemRef { library, key }`. Library IDs are opaque tokens, not
paths or citation keys. A private `library-ids.key` makes them repeatable on
one computer. File IDs follow canonical paths; Zotero IDs identify the profile
and selected personal or group library across live and closed reads. If the
key cannot be stored, IDs last only for that process. `doctor` reads the key
without creating or repairing it.

Snapshots are immutable and share unchanged objects. Each object has a
`cite` fingerprint for citation data and a `full` fingerprint for all shown
data. Search metadata can change without invalidating a citation. Citation
keys retain their source and aliases; conflicting or ambiguous keys are
refused for key output.

## Sources

The [compatibility guide](compatibility.md#library-files) lists file formats
and their limits. Files are bounded to 64 MiB of UTF-8 text. Parsers report
skipped records without discarding valid neighbours. Duplicate identities
make all records under that identity unavailable.

Zotero reads use the same pinned Zotero-to-CSL mapping in both modes:

- **Running:** the local API. Zotero 10 reconciles item versions and fetches
  changed records; Zotero 7–9 reread records. A changed API instance or
  backwards counter forces a full read.
- **Closed:** copy the database and its WAL or journal to a private directory,
  check that the source stayed unchanged, run SQLite's quick check, read the
  copy and remove it. The live database is never opened. Changed copies are
  retried at most three times. Schemas newer than the tested ceiling read with a
  source warning after required table, column and lookup shapes validate;
  incompatible layouts are refused.

A choice names the personal library or specific groups. Missing groups fail
instead of falling back. Several closed profiles require a named choice.
Keys come from Zotero's key field, Extra or Better BibTeX; disagreements
remain visible and block key output.

Long-lived file readers accept a change after two identical reads. Watchers
recover parent-directory registrations after replacement. Network and unknown
mounts use manual refresh and report `live-refresh-off`; opening the picker
rechecks these and failed sources. There is no periodic filesystem retry.
One-shot CLI reads install no watches.

Known cloud placeholders are refused before reading, without requesting a
download. Filesystem probes have a two-second wait and eight-call limit.
Desktop loads have a two-minute deadline and at most four loads per source
family in flight. Blocked OS calls can outlive the wait, but their late results cannot
publish. A failed refresh keeps the previous snapshot and reports its health.
New selections and publications retire stale candidates and their watchers.

## Citations

One `CitationRequest` holds ordered items, fingerprints, locators, prefixes,
suffixes, forms, output and locale. Repeated references are allowed. The
engine returns text, HTML/RTF where supported, display markup, per-item
outcomes, losses, notices and a content hash. Unsupported forms are refused;
fields an output cannot carry produce typed losses.

Key syntax capabilities live in `ibis_core::syntax::CAPABILITIES`. The engine's
`outputs` list supplies IDs, names and supported forms to CLI, MCP, menus and
settings. Formatted output uses citeproc-js on one worker with a bounded
queue, cancellation, time and memory limits. The compressed style bundle
loads the requested style and locale. Formatting receives citable fields,
without notes, annotations or private links. Zotero differential fixtures
check output independently of the renderer.

The desktop warms its saved default style, locale and runtime in the background,
independently of the library, and shares that formatter for the session. Warm-up
publishes no citation; a foreground request retires it. The idle worker sleeps
while Ibis is hidden.

Detail examples and bibliography work own separate cancellation tokens; closing
or replacing their view stops formatting and leaves the receipt usable. MCP
sends no result after its request is cancelled.

A prepared citation pins its request and fingerprints. Copy re-cites it
against current data and requires the shown content hash. Changed or removed
items, ambiguous keys and changed Zotero generations refuse the copy.
`copy_content(expected_items)` also requires every requested item to succeed.

Receipts, `--strict` and caller-supplied records are specified in the
[CLI reference](cli.md#receipts-and-strict-citations); records open no library
and never combine with `--keep-order`.

The locator parser is shared by search, the picker's Page field and CLI.
It recognises all 29 CSL locator terms, ranges and quoted literal labels.
A typed label overrides the field's default type; an incomplete label is an
error. Canonical locators stay distinct from raw editor text.

A writing is an optional name, format and read-only `.bib` in `settings.json`
([decision 0042](decisions/0042-formats-apps-and-writings.md)). The picker remembers
one per app; unidentified targets use session-only selection. CLI/editor context
selects one explicitly, including a unique match from supplied document front
matter; no client guesses. Each citation uses the saved definition. Failed saves
keep the old writing, format and bibliography together.

Bibliography checks are advice unless a key is known to identify another work.
For key outputs, `--strict` also requires a complete current read finding every
emitted key unambiguously. Captured definitions and monotonic activation identities
retire stale choosers, previews and delivery proofs, including A → B → A; final
context checks guard clipboard commits. Receipts include an opaque definition
hash and bibliography evidence, never paths. Writings grant no paste authority.
See [writing commands](cli.md#writings).

<a id="task-contexts-planned"></a>
Cross-client task-context plans are in [the roadmap](../ROADMAP.md#consolidation-now-alongside-finishing-03).

## Desktop app

The interface uses React, React Aria controls and a pure citation-session
reducer read through `useSyncExternalStore`. A headless controller and native
adapters own I/O with session and operation identities. Search and preview
cancellation reaches engine workers; late completions cannot replace newer
work. Library notifications use channels rather than polling.

`locales/en-US.ftl` supplies interface text and messages for the CLI, MCP and
native app. Generated bindings and output lists come from Rust; front ends
do not maintain separate output vocabularies.

`ibis_engine::settings::Owner` serializes settings changes; engine settings hold
the cross-process lock and perform atomic saves. Backup validation and preparation
are shared; desktop owns dialogs and single-use restore previews.

The engine chooses one output: the selected writing, the app's remembered format, the person's
format for its kind (documents or keys), then a suggestion, naming its source.
First run asks nothing; the first successful suggested paste remembers the
app's format and reports it once per kind. Delayed delivery cannot overwrite
newer choices. Formats per app live in `settings.json`; paste choices live in
`apps.json`. Nothing polls while hidden. Storage and data flows are documented
in [PRIVACY.md](../PRIVACY.md).

Native copy writes the prepared result. Paste uses a one-use proof tied to
the target, picker presentation and clipboard generation:

- **macOS:** requires Accessibility and paste-event authorization. Captured
  document and input-surface identities must still match. Secure input,
  intervening key/button input, changed clipboard, expired proof or uncertain
  focus causes copy with a reason. Identity checks read no document titles or
  field values. Selected text is read separately for search. Foreign-window
  checks and posting are sequential, so programmatic changes inside one
  surface are not fully detectable.
- **Windows:** captures HWND, process and integrity level. A guarded Ctrl+V
  requires that target in front, an unchanged clipboard sequence and no held
  keys. Elevated targets, password fields and uncertain checks copy instead.
- **Linux:** copies and reports why; the person pastes.

Paste refusals do not retry; permission recovery needs a fresh summon and
Enter. Platform coverage is in [compatibility](compatibility.md).

## Performance checks

These are implemented checks, not measurements of every released app:

| Check | Current limit and scope |
| --- | --- |
| `scripts/picker-budget.mjs` | Entry JS, static imports and CSS total at most 400 KiB gzipped; lazy imports and other windows are excluded. |
| `ibis-engine/tests/library_memory.rs` | Heap accounting at 10,000 generated records: 48 MiB for BibTeX, 49 MiB for CSL-JSON. |
| `ibis-engine/tests/refresh_budget.rs` | 100,000 generated records; per-query search p95 ≤ 20 ms, open/refresh < 3 s. Timing failures enforce only with `IBIS_ENFORCE_BUDGETS`; otherwise report. Ranking and correctness assertions still fail. |
| `startup_format_breakdown` (CSL and desktop unit tests) | Invented references, cold formatter phase breakdown and 30 warm samples; desktop also separates settings, a 10k library read and output catalogue lookup. A warmed first preview and warm formatting p95 have a 100 ms limit, enforced only with `IBIS_ENFORCE_BUDGETS`; shared runners report. |

Run the ignored engine cases explicitly in release mode, one test thread:

```sh
CARGO_TARGET_DIR="$PWD/target" cargo test -p ibis-engine --locked --release --test refresh_budget -- search_budget --ignored --nocapture --test-threads=1
CARGO_TARGET_DIR="$PWD/target" cargo test -p ibis-engine --locked --release --test refresh_budget -- open_refresh_budget --ignored --nocapture --test-threads=1
```

Run the test-only startup harness in separate processes for cold and warmed
desktop starts; it reads no personal settings or libraries.

```sh
CARGO_TARGET_DIR="$PWD/target" cargo test -p ibis-csl --locked --lib startup_format_breakdown -- --ignored --nocapture --test-threads=1
CARGO_TARGET_DIR="$PWD/target" cargo test -p ibis-desktop --locked startup_format_breakdown -- --ignored --nocapture --test-threads=1
IBIS_MEASURE_WARM=1 CARGO_TARGET_DIR="$PWD/target" cargo test -p ibis-desktop --locked startup_format_breakdown -- --ignored --nocapture --test-threads=1
```

### Native release measurements (#1369)

`node scripts/release-budgets.mjs evidence.json RELEASE_SHA` validates supplied
native measurements; it does not collect them. It requires the exact revision,
a named machine, cold/warm cases at 10k and 100k references, 60 seconds settled
and at least 30 samples per case. It reports p50/p95/p99 and checks:

- open p95 ≤ 100 ms; typing p95 ≤ 50 ms;
- every idle sample ≤ 0.2% of one CPU core and ≤ 1 wakeup/s;
- whole-process-tree memory ≤ 200 MiB at 10k and < 400 MB at 100k.

Debug smoke timings do not satisfy it. Collection is
[roadmap work](../ROADMAP.md#10-stable).

## Vocabulary

A **deep module** hides much behaviour behind a small **interface**; a **seam**
is where two modules meet and behaviour can be swapped. Apply the **deletion
test**: if removing a module only moves its complexity to its callers, it earns
its place. One adapter is a hypothetical seam; add the seam when a second
exists.
