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

Errors and citation refusals carry a stable code, a catalogue message id with
arguments, exactly one `Recovery` action and a `FocusTarget`. Core maps every
`ErrorCode`, `ErrorReason` and `CitationError` exhaustively; a new variant needs
a recovery before it compiles. The JSON envelope, refusal outcomes, source-health
refresh failures, MCP tool errors and native command errors carry these facts.
Desktop status also carries `failure` and `retained_failure` alongside its
rendered status text; preview refusals carry `failure`. Clients render the
message from `locales/en-US.ftl` and dispatch the declared recovery.

Generated TypeScript treats `ItemRef.key` as an opaque `RecordId`, incompatible
with strings and React children. Display labels and citation keys are separate.
Format previews use prepared engine text; another output shows its labeled
example until a prepared result is available. MCP fallback refs are opaque
digests resolved against the current snapshot, never embedded lookup IDs.

| Error or refusal | Recovery | Focus |
| --- | --- | --- |
| Missing key; unknown style/output; unsupported output; conflicting/invalid key; record not from Zotero | ChooseStyle | FormatPicker |
| Unsupported citation form | ChooseForm | CitationForm |
| Missing library file, including a failed watched refresh | LocateFile | LibraryPicker |
| Missing Zotero/profile; unsupported file format/CSV template; non-local selection | ChooseLibrary | LibraryPicker |
| Zotero sharing off | OpenZoteroSharingHelp | ZoteroSharingHelp |
| Zotero requires an open app | OpenZotero | RecoveryAction |
| Duplicate key within a library | ReviewLibrary | SourceFile |
| Ambiguous lookup key | SelectReferences | Search |
| Ambiguous citation key | ChooseStyle | FormatPicker |
| Strict citation or damaged entry | ReviewLibrary | LibraryPicker / SourceFile |
| Missing/changed item, annotation mismatch, empty selection, uncitable item | SelectReferences | Results |
| Not found | SelectReferences | Search |
| Resource limit, too many items, oversized output, cut-short library | ReduceSelection | Search / Results |
| Invalid locator | EditCitation | Locator |
| Usage, bad input, rejected/duplicate caller records, tool validation | EditInput | Request |
| Invalid rich text | EditCitation | Citation |
| Unknown/invalid language | ChooseLocale | Language |
| Source/formatter/internal failure; app did not answer | Retry | RecoveryAction |
| Newer format; missing/wrong Ibis app | UpgradeIbis | RecoveryAction |
| Zotero has no space for a private copy | FreeDiskSpace | RecoveryAction |
| Snapshot changed | SelectReferences | Results |
| Cancelled / library loading | None (Cancelled / InProgress) | Search |
| Invalid/duplicate project name | RenameProject | ProjectName |
| Missing project / project limit | ChooseProject / RemoveProject | ProjectMenu |
| Changed project or destination | ReopenProjectMenu | ProjectMenu |
| Pending editor request | FinishEditorRequest | Citation |
| Invalid project definitions | ReviewSettings | Settings |
| Invalid or read-only settings / settings from a newer Ibis | ReviewSettings / UpgradeIbis | Settings / RecoveryAction |

Native command infrastructure has a closed operation enum with its own catalogue
messages. Backup recovery chooses a backup, downloads it, retries or upgrades;
shortcut recovery chooses a shortcut, retries or opens Ibis. The command registry
wraps these failures at the IPC boundary and tests every generated error schema.

## Sources

File formats and limits are in [compatibility](compatibility.md#library-files). Files are bounded to 64 MiB; parsers report skipped records without discarding valid neighbours.

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

Missing Zotero groups fail instead of falling back; several closed profiles require a named choice. Disagreeing citation keys stay visible and block key output.

Readers accept a change after two identical reads; network and unknown mounts use manual refresh and report `live-refresh-off`. Cloud placeholders are refused before reading. Each file load shares one two-second filesystem deadline; calls share eight slots and optional attachment batches can hold at most four slots and skip immediately when capacity is unavailable, leaving availability `unknown`. Desktop loads carry the request's cancellation token through every file open and have a two-minute deadline, four per source family. Decoding is pure: the source probes attachments in one batch under the load's deadline, capped at 256, and unchecked ones stay with `unknown` availability. A failed refresh keeps the previous snapshot and reports its health; stale results cannot publish.

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

`engine.citations()` owns a bounded registry: one picker session and up to 64
connection sessions. Submission reserves order before workers run. A newer
preview retires the session's previous preview, its forms and queued
bibliography work. Running bibliography work keeps its own cancellation owner;
closing a session stops all its work. Connections close their registry entries.
One-shot RPC/MCP cite calls reserve independent operations and answer every
accepted request; interactive preview supersession does not swallow their replies.
Session failures carry a typed `Problem` with its catalogue message, recovery and
focus. BibTeX and DOI links use the same preview and checked commit as citations;
record exports pass through `CitableFields`.

The desktop warms its saved default style, locale and runtime in the background,
independently of the library, and shares that formatter for the session. Warm-up
publishes no citation; a foreground request retires it. The idle worker sleeps
while Ibis is hidden.

Desktop drafts, previews, display trees, locators and all six citation forms
come from core through generated bindings. Output and locale resolve together
before item formatting; the formatter receives the validated style and bundled
locale. Human CLI record renderers receive only presentation rows made from
`RecordSummary` and citation-key status, without lookup identities or raw JSON.

A prepared citation pins its request and fingerprints. Commit re-cites it
against current data and requires the shown hash. Changed or removed items,
ambiguous keys and changed Zotero generations refuse it, including a changed
record that would produce identical text. Checked content retains its session
identity and the citable revision checked during commit; the app holds the engine's final freshness guard while writing the
clipboard on the main thread. Clipboard and paste proof remain native-owned.
Bibliographies name unrenderable records in `left_out` and retain the rest;
their shown receipt pins the selected records, including those left out.

Receipts, `--strict` and caller-supplied records are specified in the
[CLI reference](cli.md#receipts-and-strict-citations); records open no library
and never combine with `--keep-order`.

One locator parser serves search, the Page field and the CLI (all 29 CSL terms, ranges, quoted labels; a typed label overrides the default type).

A writing is an optional name, format and read-only `.bib` in `settings.json` ([decision 0042](decisions/0042-formats-apps-and-writings.md)); the picker remembers one per app, and CLI/editor clients select one explicitly. Writings grant no paste authority and receipts never carry paths. Bibliography checks, `--strict` and stale-identity rules are in [writing commands](cli.md#writings).

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
