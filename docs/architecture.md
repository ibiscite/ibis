# Architecture

Ibis has a Rust engine, a command line and a Tauri desktop app. This page
describes this checkout. Plans live in [ROADMAP.md](../ROADMAP.md).

## Crates and clients

| Location | Responsibility |
| --- | --- |
| `crates/ibis-core` | Shared model, citation requests/results, key syntaxes, search and delivery rules. |
| `crates/ibis-sources` | Read-only file and Zotero readers (`zotero_local`: the local API, closed database and Better BibTeX), discovery, database snapshots and file watches. Each reader declares what its library offers. |
| `crates/ibis-csl` | citeproc-js in QuickJS, ICU4X collation, bundled CSL styles and locales. |
| `crates/ibis-engine` | Typed methods, source lifecycle, prepared citations, settings owner, recents, JSON-RPC and MCP. |
| `crates/ibis-cli` | The `ibis` executable, and the PDF text extractor it starts as its own child process (`ibis __extract-pdf`, with `lopdf`). Depends on core and engine. |
| `apps/desktop/src-tauri` | Native windows, clipboard, paste, shortcuts, settings dialogs and updates. Depends on core, engine and its command macro crate. |
| `apps/desktop/src` | React interface, shared controls in `ui/`, tokens in `tokens.css`. |

The desktop manages `ibis_engine::library::Owner`: commands submit intents and
read its current library or view. The owner publishes library status; the app
adds its settings notice and schedules picker and tray effects on the main thread.
Every command is `async` unless marked `#[command(main)]`, which answers at IPC
receipt and never waits; settings reads use the owner's published view
(`settings::Owner::view`), so the picker never waits behind a settings write.

The desktop links the engine in process, with generated TypeScript bindings;
it is given no PDF reader, so PDF parsing never runs in the app ([0055](decisions/0055-agent-layers-full-text-writes-remote.md)).
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
message from `locales/en-US.ftl` and dispatch the declared recovery. Each
error's recovery and focus are in `crates/ibis-core/src/contract/recovery.rs`.

Generated TypeScript treats `ItemRef.key` as an opaque `RecordId`, incompatible
with strings and React children. Display labels and citation keys are separate.
Format previews use prepared engine text; another output shows its labeled
example until a prepared result is available. MCP fallback refs are opaque
digests resolved against the current snapshot, never embedded lookup IDs.

Native command infrastructure has a closed operation enum with its own catalogue
messages. Backup recovery chooses a backup, downloads it, retries or upgrades;
shortcut recovery chooses a shortcut, retries or opens Ibis. The command registry
wraps these failures at the IPC boundary and tests every generated error schema.

## Sources

Every reader returns one `LoadedLibrary`: its id, kind, the capabilities it
declares, an immutable snapshot and warnings. The engine asks a library's
capabilities, never its kind, whether agents may read its attachment text or
propose changes to it; the kind only chooses which reader serves a request.
BibTeX named as such declares proposals; BibTeX and CSL-JSON declare native
keys and attachment text; RIS and CSV declare records only; Zotero declares
its keys, aliases, conflicts and attachment text in both read paths. Each
library's `revision` is its snapshot's full fingerprint.

File formats and limits are in [compatibility](compatibility.md#library-files). Files are bounded to 64 MiB; parsers report skipped records without discarding valid neighbours.

Zotero reads use the same pinned Zotero-to-CSL mapping in both modes:

- **Running:** the local API. Zotero 10 reconciles item versions and fetches
  changed records; Zotero 7–9 reread records. A changed API instance or
  backwards counter forces a full read.
- **Closed:** reuse one private file set in the per-user app cache, shared by
  the desktop and CLI. Refresh when the database, sidecars or Better BibTeX
  files change in size, modification time or sampled SHA-256 fingerprint.
  Refresh stages and flushes private files, checks the source stayed unchanged,
  then renames the set with its stamp published last under one cache lease.
  SQLite checks and reads only the cache; the live database is never opened.
  Changed copies are retried at most three times ([5A](decisions/0006-sources.md#closed-zotero-cache-5a)).
  Schemas newer than the tested ceiling read with a
  source warning after required table, column and lookup shapes validate;
  incompatible layouts are refused.

The source path owner supplies the OS cache directory; the reader creates
`ibis-zotero-closed` beneath it. All files use core's private creator (0600 on
Unix; inherited per-user cache security on Windows). An existing lock is never
deleted; a Windows lock Ibis cannot open reports its own error with the
"Open Zotero, then reload" recovery.
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
loads the requested style and locale. Formatting, MCP reference views and record
writers require immutable `CitableFields`, constructed only by the privacy
filter: no notes, annotations, private links or device paths in values. Source
identities and citation keys stay separate from metadata. Zotero differential fixtures
check output independently of the renderer.

The decoded projection checks visible text and markup attributes before keeping
any rich-text runs. `PublicUrl` owns URL parsing and refuses local names and
special-purpose literal addresses (IPv6 requires native global unicast).
`DoiLink` owns DOI identity and resolver links; dot path segments are refused
before any serializer, including citeproc. BibTeX identifiers and keys use
verbatim values, with balanced literal braces and no TeX commands; prose keeps
TeX escaping. Names are normalized once with both particles retained. When
both occupy one name, BibLaTeX's single prefix holds the dropping particle and
the protected family includes the non-dropping particle, keeping particles
out of given-name initialization. This keeps every name word; CSL-JSON is
required to preserve both explicit particle roles on reimport.

`engine.citations()` owns a bounded registry: one picker session and up to 64
connection sessions. Submission reserves order before workers run. A newer
preview retires the session's previous preview, its forms and queued
bibliography work. Running bibliography work keeps its own cancellation owner;
closing a session stops all its work. Connections close their registry entries.
One-shot RPC/MCP cite calls reserve independent operations and answer every
accepted request; interactive preview supersession does not swallow their replies.
Session failures carry a typed `Problem` with its catalogue message, recovery and
focus. BibTeX and DOI links use the same preview and checked commit as citations;
record exports pass through `ibis_core::citable::share_fields`
(`crates/ibis-core/src/citable.rs`), which drops notes, annotations and device links.

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
The picker submits `deliver_citation` once for Enter, with its checked preview
and native presentation. Rust re-cites, commits the copy and decides dismissal
in one main-thread task, then pastes and publishes the receipt. It returns one
typed outcome: pasted, copied-only with its native reason, retained in the
picker, sent to an editor, refused with a `Problem`, or cancelled. Pins and
warnings retain the picker; receipt
tokens stay in Rust, and inline confirmations receive its display summary.
Bibliographies name unrenderable records in `left_out` and retain the rest;
their shown receipt pins the selected records, including those left out.

Receipts, `--strict` and caller-supplied records are specified in the
[CLI reference](cli.md#receipts-and-strict-citations); records open no library
and never combine with `--keep-order`.

One locator parser serves search, the Page field and the CLI (all 29 CSL terms, ranges, quoted labels; a typed label overrides the default type).

A writing is an optional name, format and read-only `.bib` in `settings.json` ([decision 0042](decisions/0042-formats-apps-and-writings.md)); the engine remembers one per app, and CLI/editor clients select one explicitly. `ibis_engine::app_context::Owner` accepts one typed `ContextRequest` and returns the writing, format and bibliography observation together for the captured app. It owns writing edits, session-only selection, bibliography reads and stale-context refusals. The desktop adds native destination names and delivery facts, opens file dialogs and writes the clipboard. Editor requests carry their own context and never borrow the captured app's bibliography. Writings grant no paste authority and receipts never carry paths. Bibliography checks, `--strict` and stale-identity rules are in [writing commands](cli.md#writings).

<a id="task-contexts-planned"></a>
Cross-client task-context plans are in [the roadmap](../ROADMAP.md#next).

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

`crates/ibis-engine/tests/perf.rs` measures what a person feels, on the engine
path the desktop calls, with invented BibTeX libraries of 10,000 and 100,000
references. Each case runs in a fresh release process:

- `open_ms`: a new engine reads the file until it can search;
- `first_search_ms` and `summon_ms`: the first query after summoning the
  picker, and the recently added rows it shows first;
- `typing_p50_ms` and `typing_p95_ms`: every prefix of five queries, 252
  searches;
- `first_cite_ms`: the first citation on a formatter warmed at startup, as the
  app does it, and `format_p50_ms` over 30 warm two-reference citations;
- `refresh_ms`: re-reading the edited file while copies keep committing;
- `rss_mib` and `idle_cpu_percent`: resident memory after opening and
  searching, and CPU over 10 s with the library open and watched.

Window paint, the webview and the desktop command layer are outside it; read
from the code, the desktop adds a few cloned library summaries and 50 row
conversions per search.
`scripts/picker-budget.mjs` limits the picker's entry JavaScript and CSS to
400 KiB gzipped, and `ibis-engine/tests/library_memory.rs` counts heap bytes
at 10,000 records.

`scripts/perf.mjs` owns every limit:

```sh
node scripts/perf.mjs run                 # medians of three rounds
node scripts/perf.mjs check windows       # this machine class's budgets
node scripts/perf.mjs compare origin/main # head against a base, same machine
```

**What fails.** Release checks run on every pull request, push and nightly. A
pull request runs `compare` against its base commit on the same runner: both
sides build with the pull request's harness and run in alternating rounds. A
metric fails when its median is more than 20% slower than the base **and**
slower by more than a fixed floor (2–3 ms for searching, 10 ms for citing,
30 ms for warming the formatter, 50–75 ms for opening and refreshing, 15 MiB
of memory) in both the first comparison and a second comparison that starts
with the opposite build. This reduces the effect of short bursts of shared
runner load. When the build inputs are unchanged or the base cannot build or
run the first comparison, the pull request is measured alone. A failure running
the pull request's harness fails the check. Every measured batch must also
meet the `shared` budgets, and pushes and nightlies `check shared`.

Budgets are 2× a fixed machine's median and at least 3 ms over it (a second
session on the Windows laptop ran twice as slow with background work), memory
1.2×, idle 0.5% of a core, and never above what a person notices: typing
50 ms; first search, first citation and warm formatting 100 ms. Shared-runner
budgets are 2.5× the hosted median and at least 5 ms over it, memory 1.2×,
idle 1%. Run `check` on an idle machine; it prints the load average.

Two runs of the same engine on the Windows laptop differed by at most 11% on
the judged metrics; a build that sleeps 5 ms in each search fails `compare`.

**Measured 2026-11-18**, medians of three rounds:

| Metric | Windows 10k | Windows 100k | Hosted 10k | Hosted 100k | Linux desktop |
| --- | --- | --- | --- | --- | --- |
| Open | 586 ms | 5.83 s | 249 ms | 2.58 s | pending |
| First search | 1.8 ms | 14.5 ms | 0.8 ms | 10.1 ms | pending |
| Recent rows | 0.2 ms | 1.7 ms | 0.1 ms | 1.9 ms | pending |
| Typing p50 / p95 | 1.3 / 2.0 ms | 15.5 / 22.2 ms | 0.7 / 1.1 ms | 5.6 / 9.6 ms | pending |
| First citation (warmed) | 19 ms | 21 ms | 11 ms | 14 ms | pending |
| Warm two-reference citation p50 | 51 ms | 55 ms | 30 ms | 32 ms | pending |
| Formatter warm-up at startup | 205 ms | 199 ms | 115 ms | 113 ms | pending |
| Refresh | 781 ms | 5.57 s | 459 ms | 2.40 s | pending |
| Resident memory | 45 MiB | 374 MiB | 51 MiB | 433 MiB | 52 / 432 MiB |
| Idle CPU | 0% | 0% | 0% | 0% | 0% / 0.1% |

Windows is a 4-core i7-7700HQ laptop on Windows 10, quiet. Hosted is GitHub's
`ubuntu-24.04` runner in a release check. The Linux desktop has 8 threads and
was measured at load averages of 8–48, so only its memory is recorded. Both
`check linux` and `check mac` fail with the list of pending budgets. Use `run`
to collect measurements for either system.

Two numbers miss earlier targets that nothing enforced: opening or refreshing
100k references takes about 6 s on the Windows laptop against 3 s, and Linux
memory at 100k is 433 MiB against 400 MB. The budgets stop both getting worse;
they do not accept them. A profile of opening shows one thread doing the
BibTeX work (converting entries about half, parsing a quarter, resolving
crossrefs a sixth) and then the snapshot's per-record fingerprints. Converting
entries on several threads gave the same records but no faster open on the
Windows laptop. A two-reference citation takes 2.5× a single one: each of
citeproc's text, HTML and RTF passes costs four to five times more with the
second reference.

## Vocabulary

A **deep module** hides much behaviour behind a small **interface**; a **seam**
is where two modules meet and behaviour can be swapped. Apply the **deletion
test**: if removing a module only moves its complexity to its callers, it earns
its place. One adapter is a hypothetical seam; add the seam when a second
exists.
