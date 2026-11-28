# Architecture

Ibis has a Rust engine, a command line and a Tauri desktop app. This page
describes this checkout. Plans live in [ROADMAP.md](../ROADMAP.md).

## Crates and clients

| Location | Responsibility |
| --- | --- |
| `crates/ibis-core` | Shared model, citation requests/results, key syntaxes, search and delivery rules. |
| `crates/ibis-sources` | Read-only file and Zotero readers (`zotero_local`: the local API, closed database and Better BibTeX), discovery, database snapshots and file watches. Includes the built-in EndNote [source adapter](source-adapters.md). Each reader declares what its library offers. |
| `crates/ibis-csl` | citeproc-js in QuickJS, ICU4X collation, bundled CSL styles and locales. |
| `crates/ibis-engine` | Typed methods, source lifecycle, prepared citations, settings owner, recents, JSON-RPC and MCP. |
| `crates/ibis-cli` | The `ibis` executable, its same-user daemon and the child processes for PDFs, documents and built-in source adapters. Depends on core and engine, and enables the EndNote reader and the document readers in `ibis-sources` for those processes. |
| `crates/ibis-native` | Native paste with no window toolkit: the target capture, its proofs and their platform calls, and every `unsafe` block they need. The host supplies windows, the paste setting and the keyboard layout. Depends on core only. |
| `apps/desktop/src-tauri` | Native windows, clipboard, paste delivery through `ibis-native`, shortcuts, settings dialogs and updates. Depends on core, engine, native and its command macro crate. |
| `apps/desktop/src` | React interface, shared controls in `ui/`, tokens in `tokens.css`. |

The desktop manages `ibis_engine::library::Owner`: commands submit intents and
read its current library or view. The owner publishes library status; the app
adds its settings notice and schedules picker and tray effects on the main thread.
Settings-dependent state is created after the plugins and before the event
loop runs (`create_state` in `main.rs`), so no handler, command or second
launch can read it before it exists; setup starts effects. Handed-over
launches are answered one at a time, in order, by `instance::Handoff`: one that
arrives during setup or while another is answered waits its turn.
Every command is `async` unless marked `#[command(main)]`, which answers at IPC
receipt and never waits; settings reads use the owner's published view
(`settings::Owner::view`), so the picker never waits behind a settings write.

The desktop links the engine in process, with generated TypeScript bindings;
it is given no PDF reader, so PDF parsing never runs in the app ([0055](decisions/0055-agent-layers-full-text-writes-remote.md)).
Documents (manuscripts for `check` and `export`) are parsed only in `ibis __document`, the
command line's child; the readers sit behind the `documents` feature of `ibis-core` and
`ibis-sources`, which only `ibis-cli` enables, and a desktop test proves them absent ([0060](decisions/0060-one-document-pipeline.md)).
Contracts for the CLI, JSON-RPC and MCP are in [the CLI reference](cli.md); the
desktop also provides the [editor endpoint](parity.md#better-bibtex-cite-as-you-write).
Library results carry `source_health` from the state they read; later refresh
failures cannot change it.

## Warm local engine

The command line and MCP share a background `ibis daemon` process under
[decision 0065](decisions/0065-one-warm-engine-thin-shells.md). A client starts
it on demand. The public socket protocol uses the same JSON-RPC dispatcher
and method table as `ibis serve --stdio`. The CLI uses the engine's typed
replies for its terminal output. Each `ibis mcp` still speaks MCP over stdio
to its AI app and relays that session to the daemon's existing MCP runtime.

- Linux uses a `0700` directory at `$XDG_RUNTIME_DIR/ibis`. macOS uses a
  `0700` directory at `$TMPDIR/ibis-<uid>` with a short `/tmp` path when the
  socket name would exceed `sun_path`. Each end checks the peer UID with
  `SO_PEERCRED` on Linux and `getpeereid` on macOS.
- Windows uses a local named pipe with `FILE_FLAG_FIRST_PIPE_INSTANCE`,
  `PIPE_REJECT_REMOTE_CLIENTS`, a DACL for the current user SID and a token
  SID check on each end. [Win32 named pipe access rights](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights)
- The configuration directory distinguishes profiles that use different
  settings. A startup lock serializes launches. The daemon holds a separate
  lock for its lifetime. Both ends exchange a protocol version and build
  identity before dispatch. A newer client asks an older daemon to finish
  accepted work, then starts the replacement. Clients reconnect with backoff
  after a crash. A lost reply to a write leaves its outcome uncertain.
- The engine stays warm while a client is connected and exits after 60
  seconds without clients. That interval covers short terminal workflows and
  keeps residency bounded under [0032](decisions/0032-release-gates.md).
  On Unix the parser supervisor uses a pipe to end the process group even
  after a hard crash.
  Windows uses the existing kill-on-close Job Objects.

The daemon retains up to four recent CLI and MCP library selections so
matching clients reuse the same index. Different selections see their own
records and health, even when they share parsed file records. File opens check
the current bytes before reuse. Folder libraries and files with attachments
are reread on open so changes to their other files are visible. The process
shares the formatter and keeps citation sessions scoped to their connections.
CLI and MCP source adapters start in the invoking process's working directory.
Direct socket clients supply absolute file paths.

The daemon owns CLI settings writes, watches settings files for hand edits
and clears stored recents when the saved switch is off. The Tauri app keeps
its in-process engine: native shells replace it system by system, and it is
not rebuilt as a client. It shares the daemon's files instead of its memory.
Settings and recents use the settings file lock. Paste approvals in
`apps.json` belong to `ibis_engine::grants`, which keeps no copy in memory:
every answer reads the file and every change rewrites it under
`apps.json.lock`, so two processes never undo each other's choices.

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
message from the catalogue in `locales/` and dispatch the declared recovery. Each
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

[Source adapters](source-adapters.md) are separate programs that read a
manager Ibis has no reader for and answer one whole-library read over stdio
(`ibis_core::contract::adapter`). The engine resolves built-in names and
the absolute executable paths approved in the person’s device configuration.
`ibis_sources::adapter` bounds the whole read to 64 MiB and two minutes, and
keeps identity (a keyed digest of the adapter's name and `scope`), CSL
validation, the private-link filter and key claims to itself. Adapter
libraries declare records, and keys only when the adapter says it has them.
They are read on open and on reload; a failed read keeps the last good one.

A folder of PDFs is a library of kind `folder` whose records are its
`ibis-references.bib`, read by the BibTeX reader with BibTeX's capabilities
except proposals ([0061](decisions/0061-a-folder-of-pdfs-and-one-resolve.md)).
`ibis_sources::folder` lists the PDFs, and a read warns `pdfs-not-kept` when
one has no entry. Only `Engine::keep_folder` writes it, appending entries for
new PDFs from what the PDF child (`ibis __extract-pdf --details`) read, through
the proposal writer's lock, change check, atomic replace and backups.

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

A pack of references (`Engine::share`, [#1953](https://github.com/ibiscite/ibis/issues/1953))
plans its files once for both the preview and the write: the reading list page
from the formatter's display tree, escaped by the pack writer with links only
from `DoiLink` and `PublicUrl`; `References.bib` from the `bibtex` output; and,
when asked, PDFs located by the source's own attachment rules with
`outside_library` off and opened by `pdf::LocalPdf`. It creates a new zip
(`zip`, stored entries) or folder and never replaces one; its answer names
files inside the pack, never a location.

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

A project (stored under `writings` in `settings.json`) is an optional name, format and read-only `.bib` in `settings.json` ([decision 0042](decisions/0042-formats-apps-and-writings.md)); the engine remembers one per app, and CLI/editor clients select one explicitly. `ibis_engine::app_context::Owner` accepts one typed `ContextRequest` and returns the project, format and bibliography observation together for the captured app. It owns project edits, session-only selection, bibliography reads and stale-context refusals. The desktop adds native destination names and delivery facts, opens file dialogs and writes the clipboard. Editor requests carry their own context and never borrow the captured app's bibliography. Projects grant no paste authority and receipts never carry paths. Bibliography checks, `--strict` and stale-identity rules are in [project commands](cli.md#projects).

<a id="task-contexts-planned"></a>
Cross-client task-context plans are in [the roadmap](../ROADMAP.md#b-04-one-contract-one-context-one-delivery).

## Desktop app

The interface uses React, React Aria controls and a pure citation-session
reducer read through `useSyncExternalStore`. A headless controller and native
adapters own I/O with session and operation identities. Search and preview
cancellation reaches engine workers; late completions cannot replace newer
work. Library notifications use channels rather than polling.

State that newer work replaces has one owner: `ibis_core::epoch::Epoch` in
Rust and `latest()` in `apps/desktop/src/latest.ts`. The owner holds the state;
work reserves a stamp when it is asked for and commits its result, success or
failure, through that stamp. Beginning newer work, hiding or closing retires
older stamps, whose updates then never run. There is no setter and no way to
ask whether work is still current.

`locales/en-US.ftl` supplies interface text and messages for the CLI, MCP and
native app; `de.ftl` and `ar.ftl` hold the same messages for the app, which
speaks one language per run ([0058](decisions/0058-interface-language.md)). Generated bindings and output lists come from Rust; front ends
do not maintain separate output vocabularies.

`ibis_engine::settings::Owner` serializes settings changes; engine settings hold
the cross-process lock and perform atomic saves. Backup validation and preparation
are shared; desktop owns dialogs and single-use restore previews.

The engine chooses one output: the selected project, the app's remembered format, the person's
format for its kind (keys or documents), then a suggestion, naming its source. Key formats come first: an app Ibis does not list is suggested the engine's default key output; Word, LibreOffice and other listed document apps a style.
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

### Daemon measurements

Measured on arm64 macOS on 2026-11-26 with a release build and the 10,000
invented references produced by `library` in `crates/ibis-engine/tests/perf.rs`.
Times include the entire `ibis search avery --file … --json` invocation.
Cold starts use a fresh daemon with the file already in the OS cache. Warm
searches reuse its loaded library and index.

| Measurement | Median | Samples |
| --- | ---: | ---: |
| Cold CLI search | 354.9 ms | 7 |
| Warm CLI search | 93.7 ms | 35 |
| One daemon RSS after searches | 64.1 MiB | 7 |
| Three concurrent `ibis serve --stdio` processes, combined RSS | 221.9 MiB | 3 |

RSS was sampled with `ps` after the same search and counts shared pages in
each process. The daemon exited 60.1 seconds after its last client disconnected.

## Vocabulary

A **deep module** hides much behaviour behind a small **interface**; a **seam**
is where two modules meet and behaviour can be swapped. Apply the **deletion
test**: if removing a module only moves its complexity to its callers, it earns
its place. One adapter is a hypothetical seam; add the seam when a second
exists.
