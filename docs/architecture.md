# Architecture

This is the target architecture for the rebuild agreed on 2026-10-26 ([decision 0028](decisions/0028-engine-first.md)). Code on `main` still follows the older design in places; anything rebuilt follows this document. Build order is in [ROADMAP.md](../ROADMAP.md).

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

- **The engine is the soul of Ibis** (maintainer, 2026-11-02). Almost no one sees it, but everything is done there and it makes Ibis powerful and fast; the app is only a layer on top. It must stand on its own, adapt and grow, work with AI agents, run on every operating system and be there for anyone to build on, developed in tandem with the app and with no compromises.
- **One contract, one door.** What Ibis promises is the contract: methods that take and return JSON, stable error codes, `ibis schema` and the golden cases. `ibis-engine` keeps that promise and is the only crate a front end should need. JSON and codes are stable; the sentences people read are free to improve. The app stays in-process because that is simpler, not because a process boundary is slow, and it uses the contract so that moving it out later is a small change.
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
- **Links** (`ibis://`) are untrusted input: they only fill in the picker and never copy, paste, change settings or read files ([decision 0030](decisions/0030-links-and-launchers.md)).
- **Speed is a feature, shown by numbers.** Every speed or memory claim has a measurement that runs. Optimise the hot path the measurement finds, not a guessed one. Limits block at the size people have and stretch beyond it with the same numbers, so a miss teaches without blocking (decision 0032).
- **Opinionated defaults, sharp knives on request.** The first run works with no setup and good choices already made. Anyone can change any choice, but nothing asks a person to configure what a good default can decide.
- **Fewer moving parts.** Prefer one process, standard formats and code Ibis already ships over new services, dependencies and custom infrastructure. When a simpler option makes code unnecessary, delete the code.
- **Long-lived foundations.** Standards and widely used open source first (CSL 1.0.2, CSL-JSON, BibTeX, SQLite, Zotero's APIs, PDF.js). Each dependency that could fade sits behind one Ibis module so it can be replaced: Tauri, React Aria, PDF.js, citeproc-js, the BibTeX parser.

## Engine direction

From the engine review of 2026-11-02. These need no further decision; each step lands with its own issue.

- **Reaching it:** `ibis ... --json` and `ibis serve --stdio` from any language; `ibis mcp` for agents; `ibis lsp` for editors (0.7); WebAssembly for websites and browser extensions. Python and npm packages, when someone asks, are thin clients around the `ibis` program. No C library, native language bindings or hosted API.
- **Portable rules:** `ibis-core` stays free of native dependencies and builds for WebAssembly today; the key-output golden cases matched byte for byte there. `ibis-sources` gets a feature switch so its readers can follow without Zotero or SQLite.
- **Fewer crates:** the app no longer depends on `ibis-zotero-local` (#1260), so it can merge into `ibis-sources`. Settings and per-app choices move out of `ibis-core`; health and `doctor` inspection now live in the engine.
- **Extending:** data files first, then pipes, never native code or scripts loaded into the engine. Extensions carry their own tests.
- **Stays:** citeproc-js in QuickJS, JSON-RPC with LSP framing, a fixed list of source kinds, one repository and one version number until 1.0.

Health and setup discovery have one engine owner (#1300, PR 4). Health reads one
published snapshot and its existing source metadata; clients supply selection
facts and format the shared verdicts. Doctor's inspection is read-only: no
watches, discovery, Zotero probes, database copies or recovery writes. Its first
identity read never saves a key; temporary IDs stay stable for that process.
Setup owns expiring native path handles, reserved before work starts; late
completion cannot publish, and only names reach the webview. Native inspection
shares source reads' two-second waits and eight-call limit. A failed choice keeps
the searched source and reports its health separately from the rejected choice.

### Task contexts (planned)

Approved 2026-11-04 ([decision 0028](decisions/0028-engine-first.md)):
saved project definitions and source updates can be reused across clients, but
the selected project, source scope and output belong to each writing task.
The picker switching from Thesis to Paper B cannot retarget an agent already
working on Thesis, and the agent cannot switch the picker by selecting Paper B.
An explicit request to use the picker's current project captures it at task
start. Search, picks, previews, destinations and pending delivery stay local to
their session. A source update still revalidates affected citations; separate
contexts do not preserve permission to cite a changed work or conflicting key.
This behavior does not require one shared running engine.

A writing project is only a `.bib` file and its format (maintainer, 2026-11-05;
decision 0028): there are no cross-client update rules and no project search
defaults. The next citation uses the project as saved.

**Recents:** one engine owner persists bounded app-scoped identities; session references, undo and last-citation text stay in memory.
Only successful clipboard writes record in memory immediately; one serialized worker persists; Quit waits at most one second for recents, then exits even if the latest save is lost. Forget/off retire pending recording; old completions cannot restore text or identities.
Unknown apps, editor requests and samples never persist. At 256 KiB, discard the oldest app; failed storage keeps memory usable. Delete obsolete webview history on startup and privacy clears, reporting failure; never import it (#1300).

## Data model (ibis-core)

- **Objects:** `Record` (a citable work: CSL-normalised fields plus citation key with aliases), `Attachment`, `Note`, `Annotation`, `Collection`, `Tag`, `SavedSearch` (name and conditions only). Each is addressed by `ItemRef { library, key }`; children point to their parent.
- **Availability** of an attachment is computed by Ibis the same way in every mode: `OnDisk`, `CloudPlaceholder`, `NotDownloaded`, `Online`, `BrokenLink`, `Unreadable`, `Unknown`. Standalone notes and files are shown without inventing a parent.
- **Library IDs are opaque:** a token of at most 64 letters, digits, `_` or `-`, with no meaning. A library's `kind` and `capabilities` are their own fields of `libraries`; nothing reads them from the ID, so a new kind of source needs no change to it. A well-formed ID that is not loaded is `not-found` and starts no reading; a malformed one is `bad-input`. File locations stay in the engine and never serialise.
- **Snapshots are immutable.** A refresh builds a new snapshot and reuses unchanged libraries. Each object has two fingerprints: `cite` (what a citation depends on) and `full` (everything shown). A picker session keeps the snapshot it started from. At copy time, in one step against the newest snapshot, each pick is checked by `ItemRef` and `cite` fingerprint and, for key outputs, its key is checked to still name exactly that one item across all loaded libraries; a pick that was deleted, changed what it cites or lost a required key to another item is refused with the reason. Formatted citations do not require a citation key. The picker shows the snapshot's age when it is old and says when a refresh failed.
- **Source identity lifecycle:** a library's ID is a digest of its source's identity and a private random key, so a seen ID confirms no guessed path, and the same person gets the same ID across runs and front ends. The key is `com.ibiscite.ibis/library-ids.key` in the app configuration folder, written once by the first app or `ibis` command that needs an ID (owner-only, and shared by Nightly); `ibis doctor` reads it but never writes it. Where it cannot be kept, IDs last for that process only, and an old ID then finds nothing. IDs are not portable between computers. A file library's ID follows its canonical path (a moved file is a new library; an edited file keeps its ID), and each chosen Zotero library (the personal library or a group, by Zotero's public group ID) keeps one ID across live and closed reads. A Zotero ID also covers its profile, so two profiles never share one. With one data directory the profile is that directory. The local API never names its directory, so when several are configured both reads go by the profile's account: a live read asks the running Zotero, and a closed read takes the chosen database's signed-in user ID, or none when its personal library has no top-level item, which is what the local API then reports. Two profiles of one account hold the same synced items and share IDs; never-synced profiles cannot be told apart, and because item keys are random, a reference from one finds nothing in the other. An unreadable key file is left alone and reported by `ibis doctor`, never repaired, because another process may be using it. A request names the libraries to read; a new choice replaces the old one in one publication once all its libraries are read, a missing group fails the read instead of falling back to another library, and Better BibTeX's local library IDs for groups are matched by unique group name (an ambiguous name means keys unavailable for that group). Default-directory discovery supplies only the closed fallback; API metadata does not prove a physical data directory. A single private source generation and ready flag retire prepared Zotero citations during an uncertain full read or detected switch; only a coherent publication makes the source ready again. The generation is captured with the render snapshot and checked again when a copy re-cites, alongside citation fingerprints and the prepared content hash.

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

- **Capabilities are a fixed list per source kind**, not a plugin system. Zotero open and Zotero closed are one kind, so they cannot declare different lists; anything only one path can provide is not offered.
- **Zotero:** Ibis maps Zotero's raw item data to CSL itself, from a pinned copy of `zotero-schema`, instead of reading Zotero's `csljson` export (it changes between releases, plugins alter it, and it is the slowest part of a scan). Both access paths fill the same neutral item.
  - **Running:** the local API (needs Zotero's "Allow other applications"). Zotero 10 reconciles its complete item-version map and fetches changed raw records; missing IDs remove deletions. Zotero 7–9 conservatively reread all records. Reload, a changed API instance or a backwards library counter rereads everything.
  - **Closed:** only when no Zotero process holds the data directory, Ibis copies `zotero.sqlite` and its WAL or journal into a private directory (owner-only permissions, excluded from backups, size-checked against free space), checks that the source did not change during the copy (size, modification time and SQLite header before and after), runs SQLite's quick check on the copy, reads it and deletes it. A changed or failing copy is retried up to three times, then refused with a reason. Leftover copies are deleted at start. The live database is never opened. Versions are told apart by schema; a database newer than Ibis knows is refused with a message. Copying while Zotero runs (for a faster start) stays off until a test with concurrent writes, checkpoints and journal recovery shows it safe.
  - **Several profiles:** `profiles.ini` can name several data directories, and a closed read refuses with `zotero-profile-needed` until one is chosen. A profile is named by its cleaned, unique name, so no location crosses into a request, a result or the web view. A named profile that is gone or unreachable refuses instead of reading another, as a missing group does. A chosen profile is a request like any other: only a closed read that used it saves it, under that request's identity. It is local to one computer and stays out of settings backups. A running Zotero answers for whichever profile it opened, and its libraries keep the IDs a closed read of that profile gives them.
  - **What a person can do:** `ErrorReason` carries `zotero-sharing-off`, `zotero-profile-needed`, `zotero-needs-opening`, `zotero-not-found` and `zotero-no-space`, on the failed request and with each library's health, so front ends choose their words and their fix from the reason, never from a generic code. Group names come from Zotero while it runs and from the last closed read while it does not; listing never starts a read.
  - **Citation keys** are read from Zotero's `citationKey` field (Zotero 8+), a `Citation Key:` line in Extra, and Better BibTeX's key store (read as a copy, through its own versioned adapter). Each key keeps where it came from. When they agree, that is the key; when they disagree, the item shows the conflict and key outputs for it are refused until it is fixed in Zotero, because Ibis cannot know which key the document's bibliography uses. Aliases come from Extra. A key that matches zero or several items is refused for that item. An item's identity is always its `ItemRef`, never its key.
  - Zotero 10's local write API exists; Ibis stays read-only until an opt-in decision says otherwise.
- **BibTeX/BibLaTeX:** each entry is parsed independently, so a broken entry produces a warning with its line while the rest loads, on open and reload alike; work is bounded so hostile files cannot hang the engine. Long-lived clients watch files and only replace them after two identical reads. A watch failure keeps the successfully read snapshot usable and reports the fixed `live-refresh-off` warning; manual Reload remains available. One-shot CLI calls install no watches.
- **CSL-JSON:** `citation-key` is the key; `id` names the record inside its file and is never a key. A record with only an `id` formats, and key output refuses it with `missing-key`. Key export writes `citation-key` beside the source `id`, unchanged, writes a record cited twice once, and refuses a second record with the same `id`, which only another library can hold. A repeated `id`, however written (`1` and `"1"` are the same), makes every record under it unavailable with a warning, as for repeated BibTeX keys.
- **RIS and the CSV template** ([decision 0041](decisions/0041-ris-and-csv-libraries.md)): no citation keys; formatted citations only. RIS is recognised by its `TY` line (`.ris`, `.txt` or any name), the CSV template by its suffix and its checked header. A record's identity is a versioned hash of the CSL data read from it, so it survives refreshes and fresh exports; damaged records are skipped by line, as for BibTeX. Each format's file names are listed once, in `ibis-sources` (`file.rs`, `NAMES`); reading, Copy Entry, project bibliographies and the app's chooser, drop and Open take them from there, so a new format is one entry and its reader.

File reads carry the canonical identity from the observation that supplied their
bytes; a symlink retarget during that observation is refused. Strict Reload reads
and prepares its index and watcher privately. Watcher callbacks wait for that
candidate's acceptance; native registration never holds the selection guard.
The current desktop request's selection guard
owns final acceptance, and any newer accepted data publication retires that candidate.
Failed Reloads record source health under the same guard, so queued notifications
cannot clear the failure or advance the last successful check.
Concurrent failed reads retain their errors for unrelated libraries; a fresh repair
clears its own error. Zotero's read guard must still match at acceptance.

### File availability and notification recovery (#1532, items 2, 3 and 7)

Native watch registration does not prove that other clients' edits generate events.
Network and unknown mounts use manual refresh and the existing `live-refresh-off`
warning. Mount classification follows the selected target: known local filesystem
types on macOS/Linux, and local volume types on Windows (including mapped-drive
classification). Opening the picker checks these sources once, as well as files
whose last refresh failed, including a desktop timeout before engine health was
updated; a newer choice supersedes that check. File checks prepare a strict
reread for guarded acceptance; Zotero keeps its normal change check. There is no
periodic filesystem retry, visible or hidden. Local notifications can still
refresh a hidden picker. Unavailable files recover on a notification, Reload or
the next picker opening.

Check Windows offline/recall attributes and macOS dataless flags before opening
file data, and check descriptor metadata again before reading. Legacy iCloud
`.icloud` placeholders are deferred too. The unavailable-file message asks the
person to download online-only files first; known placeholders are refused
without requesting hydration.
Filesystem read/probe waits have a two-second deadline and at most eight detached
calls. The desktop supervises an entire load, parsing included, with a two-minute
deadline, because a large library on a slow machine needs longer than a stuck read,
and waits for completion, cancellation or that single deadline; it does not poll.
Blocked metadata or provider I/O cannot serialize newer choices behind it.
Request reservation records the
chosen spelling without filesystem access; canonical resolution belongs to the
supervised worker. At most four complete loads can remain in flight; saturation
reports unavailability instead of queuing forever.
OS calls already blocked cannot be terminated safely; they release their slot
when they return. Late results have no publication authority and retain their
original cancellation and request identity. Reload candidates and their watchers
still require guarded acceptance before they can publish.

Watch an existing ancestor of each selected/target parent. Directory events
invalidate registrations; reinstall the parent even if its spelling is unchanged,
and move the anchor upward if several levels disappeared. A failed registration
retains the old references and `live-refresh-off` until a registered, stable read
succeeds. Ordinary leaf edits reuse registrations; replacement closes the
registration gap with two identical reads. These changes add bounded helper
threads and metadata probes, with no hidden polling. Fakes cover event-less
mounts, placeholders, blocked reads, stale ownership and registration refusal.
Native evidence still requires Linux directory replacement, a two-client network
mount, Windows provider states and iCloud provider states separately.

## Citations (ibis-core, ibis-csl)

- **Request:** ordered items (the same reference may appear twice); a citation form (normal, narrative, author only, year only, full, bibliography only) and "keep my order"; per item a typed locator (one of the 29 CSL 1.0.2 locator terms), rich-text prefix and suffix (italic, bold, sub, superscript), author suppression, and the annotation it came from if any; an output spec or named preset; an input language for parsing typed locators and an output locale.
- **Result:** text, HTML, RTF and a display tree; one outcome per item; typed losses; notices such as "needs the whole document"; a content hash that copy and paste must match.
- **Key syntaxes** are pure Rust, each with a capability table (which fields and forms it carries, its escaping, a fixture checked against its own tool): Pandoc, BibLaTeX (any command, multicite), natbib, LaTeX `\cite`, ConTeXt, Typst, org-cite, MyST, MultiMarkdown, AsciiDoc, Jupyter, Jekyll Scholar, Scannable Cite, bare keys, CSL-JSON, Better BibTeX's CAYW JSON.
- **Formatted output:** Zotero's citeproc-js runs in embedded QuickJS (`rquickjs`) with collation from ICU4X, so the command line and the app give byte-identical output on every system. One formatter runtime runs on its own thread with a bounded FIFO queue, queued cancellation and running cancellation through QuickJS's interrupt handler. Memory and time limits bound each request, including its queue wait; the runtime is restarted after an interruption. Every official style and locale ships in the binary as one compressed bundle pinned to an upstream release; a request unpacks only what it names, a dependent style runs its parent's rules in its own default language, and rules come from what a style declares, never from its name. A result states the language it is written in; the interface language has no say in it. Before formatting, Ibis applies Zotero's own data conversions (Extra fields, creators, dates, URL stripping for printed articles with pages); differential fixtures compare the result with pinned Zotero output. The formatter, and AI agents unless the person has turned notes on for them (decision 0033), then get only `ibis_core::citable` fields: note field overrides become fields, and notes, annotations and private links are dropped.
- **What a citation cannot know:** selected items (or even the whole library) cannot establish a document's numbering, year suffixes or ibid. Numeric styles give the reference entry with a notice (today's fallback), note styles give the full first note, and 2020a/2020b suffixes are withheld with a notice. Byte-identical output across front ends is necessary, not sufficient: correctness is checked against Zotero.

- **Writing context:** optional writing projects keep an engine output and a checked bibliography in one saved definition with an opaque stable ID. The desktop remembers selection per app; an unidentified destination uses a runtime-only selection. Each opening reads the saved projects, so a change made in another app is used by the next citation (maintainer, 2026-11-05). Direct task edits apply after a successful save and retire queued insertion. Definitions, selection and their edits go through the one settings owner (decision 0040), each as the core transaction under the cross-process, nonblocking file lock. The engine exposes a typed project citation result containing the ordinary citation and the exact-file check separately; explicit CLI/editor requests never inherit the GUI selection. In the picker, project generation and bibliography revision bind every native preview receipt. Switching, including A → B → A, retires earlier receipts and pending delivery proofs; the project lock spans the final context comparison and clipboard commit. Publishing changed bibliography evidence revokes even detached delivery proofs before installing the new answer, using the same project-then-delivery lock order. Native project events carry activation generation and read revision: activation retires earlier intent before its read, while a new Enter waits through opening and read settlement. In-flight copies retain their native checking-context identity, so a refreshed check cannot adopt an earlier copy; repeated identical actions in that same context join the existing operation. Bibliography reads use one parser and one latest queued choice, bounding memory during rapid switching without polling. A delayed native file chooser also carries the original project and activation generation. The shared engine check refuses a key whose entry in the newest read has a different DOI, under every policy; anything else it cannot settle, an unreadable or damaged file included, is a warning, and nothing older than the newest read is kept (maintainer, 2026-11-05). `--strict-project` also requires a complete current read. No project identifies the actual document or grants paste authority (decisions 0028 and 0031, #1697).

A result may report partial outcomes for inspection, but clipboard clients must
use `copy_content(expected_items)` to require all requested items, no refusals and
a matching content hash. An annotation pick carries its own cite fingerprint. Native clients obtain an opaque `PreparedCitation` from the engine, which pins the cite fingerprint of every picked record and annotation. A copy calls `recite`: the stored request is cited again against the current library and must give the shown content hash, so a changed or removed reference, a key another record now shares, a changed annotation file or a different Zotero is a refusal, and no separate check can drift from what citing does. The app then writes what `recite` returned; a publication after that cannot change what was true when it was checked, so no lock is held across the native write. The serialized method surface has no clipboard operation, and publication notifications use channels rather than synchronous app callbacks.
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
`styles` pages through every bundled style, or with an `id` gives one style and its credits. Human CLI citation/bibliography options,
canonical JSON requests and stdio use those same results. RPC cancellation reaches
the formatter task, and exhaustion remains a typed resource-limit error.

- **Typed engine methods** are the source of truth for the command line, `ibis serve --stdio` (JSON-RPC 2.0, as language servers use), `ibis mcp` for AI agents, and the app. Each front end lists which methods it exposes; the app keeps per-window command permissions (decision 0024), so a generic dispatcher never widens what a window may call.
- **Every `--json` output is a versioned envelope** `{ "ibis": 1, "ok": …, "data" | "error": … }` with stable error codes, including argument errors. `data` carries partial results with per-item outcomes, losses and refusals; an empty search is `ok` with no results. Only the envelope goes to stdout; logs go to stderr.
- **Exit codes** describe how a command line run ended: 0 ok (including empty results), 1 done with findings (refusals, losses, source warnings or refresh failures), 2 usage, 3 bad input, 4 a named item or library not found, 5 source unavailable or resource limit exceeded, 6 newer format than Ibis knows, 70 internal error, 130 cancelled.
- **`ibis serve --stdio`** uses JSON-RPC 2.0 with LSP-style `Content-Length` framing, library-change notifications, `$/cancelRequest`, a message size limit and nothing else on stdout. Its errors are JSON-RPC errors whose `data` carries the Ibis error code; exit codes do not apply per request. `ibis mcp` keeps MCP's own envelope over the same methods.
- **The command line ships on its own:** every nightly and release publishes `ibis` for each system as an archive (`ibis-<system>.tar.gz`, `ibis-Windows.zip`) with the licence and third-party notices, checksums and the same build provenance as the app, and the release workflow runs each archive on its system before publishing (the Intel macOS archive is cross-built and not run). The app does not install it on PATH.
- **`ibis schema`** prints the JSON Schema of every method (generated with `schemars`). The app's TypeScript types come from the same `schemars` schemas: each desktop command records its argument, result and error types (`apps/desktop/src-tauri/macros`), and `node scripts/gen.mjs` turns them, with the engine types they name, into `bindings.ts` (decision 0035). `ibis call <method>` reaches any method from a shell.
- **Bounds and invalidation:** request JSON is at most 1 MiB and request IDs at most 1,024 UTF-8 bytes. Results share a 16 MiB response budget with 8 KiB reserved for ID/envelope overhead; oversized results return `resource-limit` (exit 5). Requests execute in order on one worker behind an eight-request queue; a long request delays later requests on the same connection. Running and queued requests can be cancelled. Array frames receive a single Invalid Request error (`-32600`, `id: null`). Library notifications are bounded wake-ups; subscribers read current library state after a wake. Refresh failures retain the prior snapshot and report its last successful update time plus a fixed error code.
- **Golden cases** (a new `cases` folder in `fixtures`: library, request, exact expected result) are replayed through every front end that exposes the method and must match byte for byte.

### Cost of every style (#19)

All official styles and locales are one file in the binary, in 1 MiB zstd blocks
(decision 0007). Changing style unpacks the style list and one block, then lets
both go; formatting again in the same style unpacks nothing, and only the last
style used stays unpacked. The bundle is read from disk only when used, so an idle
or hidden app that formats nothing pays nothing. Sizes, memory and timings are
measured on the pull request that changes the bundle (#19's are on its pull request).

Your styles (decision 0007) are read from their folder when used; a request
reads at most two small files of yours (a style and its parent) and unpacks
nothing more for them.

`style_sweep` renders six invented records in every style, in every form. Author
only, year only and narrative are offered only for the styles checked against
Zotero; other styles refuse them until their output for those forms is verified.

### Large-library measurement (#1251)

The release limits are in decision 0032 (2026-10-31): they block at 10,000
generated realistic records and a 50,000-record check reports on nightly. 100k is
no longer a target. Pull request CI fails on search p95 > 20 ms and picker assets
> 400 KiB gzipped. The 100k checks below still run until #1536 moves them to the
new sizes. Each run's numbers belong on its pull request, not here.

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

Enforcement is split by measured margin (maintainer, 2026-10-28):

| Budget | Enforced in | Reason |
|---|---|---|
| Search p95 ≤ 20 ms, each query | `ci.yml` → `test` (`Test (ubuntu-24.04)`), release `search_budget` | At least 61% measured margin |
| Picker JS/CSS ≤ 400 KiB gzipped | `ci.yml` → `changes` and `web`, both picker build paths | Deterministic asset bytes |
| 100k open and refresh < 3 s each | Where `IBIS_ENFORCE_BUDGETS` is set; `preview.yml` → `release-checks` reports it on every build | Shared runners measured 2.1–4.1 s on one commit (v0.3.0, 2026-11-07) |

Preview's release checks cover pushes to `main`, tags and manual runs; the
scheduled update-test-only run skips them. On shared runners a slow open or
refresh is reported, never fatal; a failed command, ranking or refresh/copy check
still stops the build.
Both engine cases use one test thread after compilation finishes; no retries,
discarded warmups or best-of selection hide failures. These local engine runs do
not substitute for native release-machine measurements.

Refresh compares normalized records against the prior validated model before
collecting a candidate, so it fingerprints only the records that changed.
Fingerprinting batches JSON writes to SHA-256 without changing their bytes,
hashes identical cite/full views once, and computes aggregate fingerprints once per combined publication. A sole
library shares its entire object slice with the source. BibTeX refresh reuses old
records while parsing; other objects are coalesced by full fingerprint before a
watcher retains the next model. Its callback acknowledges the installed,
coalesced source snapshot; rejected publications leave the prior model intact.
Only explicitly seeded watchers keep a model. This prevents a watcher from
holding a second, pre-coalescing copy of the library. Index refresh reuses
prepared keys, identifiers and normalized text. Assembly and index preparation precede the
brief state swap.
Header deduplication tables are released before conversion, and CSL construction
moves normalized strings into JSON values instead of serializing and copying them
again.

## Desktop app

- **Shell:** Tauri 2 with the system web view; React with React Aria Components for every control (decision 0021); tokens in `tokens.css`; shared components in `src/ui/`. The app links the engine in-process and calls the same method functions; the web view never touches the clipboard, files or the network.
- **State:** the citation session uses one pure reducer read through `useSyncExternalStore` and a headless controller with session and operation identities; search/preview I/O, destination choices and preferences remain in their adapters; no state library.
- **One output vocabulary** (#1260): the engine's `outputs` list (`crates/ibis-engine/src/outputs.rs`) names every output once. The command line's `--format`, MCP's `format`, the app's menus, Settings, `apps.json` and writing projects all store and send its ids, and show its names. The web view reads the list from a constant generated into `bindings.ts`; the app keeps no output enum or name table of its own. A stored id this Ibis does not list falls back to the default output; a writing project whose output is not listed is refused in the picker.
- **One settings owner** (decision 0040): `ibis_engine::settings::Owner` applies every change to `settings.json` in the order it was asked for, one at a time, each as the core transaction. The app submits a picker choice at IPC dispatch and asks for its outcome by receipt; other writers wait their turn and may check their own operation inside the change. The command line uses the same owner. System effects stay in the app.
- **Settings backups** (#1300): `ibis-engine::settings_backup` owns bounded reads, export and restore preparation, and fixed error classification for CLI and desktop. Exports refuse absent or recovered settings. Core validates the complete backup and holds the settings writer lock across reload, preview comparison and atomic commit; the restore runs in its turn through the one settings owner. Native dialogs, single-use preview tickets and restart remain in desktop; only file names reach restore previews. Paste approvals and device-local settings stay on their computer.
- **One record summary** (#1675): `ibis_core::summary::RecordSummary` reads a record's title, authors (reading name and surname), year or date text, container and type once; the picker's rows, the command line, MCP and the search index's year all use it. Front ends word what is missing ("No date", "n.d.", "[untitled]") but never read the CSL themselves.
- **One catalogue** (#1675, #1368): `locales/en-US.ftl` holds the words every front end shows. The engine returns codes; each `ErrorCode`, `ErrorReason`, `CitationError`, warning, loss, notice and citation form has one message, named from its wire name (`error-source-unavailable`), whose value is the sentence everyone reads and whose `.cli`, `.agent` and `.app` attributes say what to do next there. `ibis_engine::catalogue` words them for the command line, MCP and the app's native side; a test fails on a code without words or words without a code. The web view reads the same file through `@fluent/bundle` for all its interface text, by id (`say("picker-…")`); `i18n/` checks the ids and variables it uses.
- **Output per app:** the destination table classifies known browser and code-editor app IDs as needing an output question, without inferring document syntax. The native destination reply carries this category alongside app ID/name, outputs and the separate app paste choice. The first copy with no remembered browser/editor output holds its citation in the picker and opens the existing chooser; a format pick or dismissal resumes it once when its preview is ready. An unrecognised app with a proven target asks for paste approval or Copy only in that same chooser; either answer cancels the held action and requires fresh Enter. Format alone never approves paste. The native copy proof freezes delivery approval before clipboard I/O; policy changes revoke outstanding epochs, and the picker cannot join flights across delivery generations. Unlisted-app approval includes an app-location fingerprint, never a raw path. Unreadable-store recovery persists through format saves and restarts; only each app's explicit delivery choice resolves it. All target, permission, lifecycle and one-use guards still apply. Search/citation changes, hide/reopen and menu commands cancel the held action. Formats per app are saved in `settings.json` through the one settings owner; delivery choices stay in the bounded, atomic `apps.json` (decision 0040); no polling or document inspection. Linux captures no app, so the default applies there. Classification and remembered choices add bounded lookups per destination/copy and no hidden activity.
- **Keys:** one command registry (id, default chord per system, user override) drives the keys sheet, footer, menus and docs. Conflicts are checked against Ibis's own keys, a reserved list per system and a real registration attempt. Escape, Tab, Enter and arrows cannot be rebound.
- **Layers, focus and commands:** each window has one layer stack and one key listener ([`ui/layers.ts`](../apps/desktop/src/ui/layers.ts)), on `window` in the capture phase, so no control can swallow a key and focus resting nowhere loses none. Esc closes the nearest layer, in one fixed order: sheet, menu, panel (the citation inspector), chooser, search text, window. A surface registers with `useLayer(kind, open, close)` and never handles Escape or stops other keys for the picker's sake. Closing returns focus to what opened the layer; if that is gone or hidden, the window's home takes it (Search, or More on the welcome, which hides Search). A key pressed with focus lost goes home first. A close is immediate: a closing modal refuses focus, so nothing waits for its exit. The picker owns its keys and commands in one place (`usePickerKeys`): a key and its menu item call the same `run(command)`. Toggles leave layers as they are; any other command clears the sheet and menus first; the inspector is a live panel, not an overlay, so it stays; a layer's own key closes it. A command that hands the screen to another window has one effect, in native code on every system and build (`picker::yield_to_window`), for the presentation that asked. Every way into Settings ends in `settings_window::present`, which closes the picker even when pinned, because a floating picker would cover it; the pin stays for its next opening. An opened source hides an unpinned picker and leaves a pinned one beside it. The picker's own file dialog keeps it, as a parent.
- **Pasting into the previous app:** a pure decision in the engine and thin per-system code. macOS captures the active app (bundle ID, process, name) and opaque document and top-level input-surface identities before the picker shows. Decision 0029 requires Accessibility access for identity-only AX reads, separately from PostEvent. The globally focused application/window and the focused element's containing document/PID must agree; AXTopLevelUIElement also identifies sheets/drawers, so they cannot borrow the document's proof. Both identities must match at delivery, including when another process has a nonactivating panel. Several windows of an app are supported without enumerating windows or reading titles, field roles, values or secure-field attributes. Each query has one 50 ms sleep-inclusive budget with per-message remaining timeouts; after refreshing authorization, clipboard and security checks, its three final document/surface/global-focus reads share at most 15 ms. Clock sampling admits brackets shorter than 500 µs, preserving interval intersection and bounding combined sampling skew below 1 ms apart from nanosecond rounding. Missing Accessibility uses `accessibility-required`; failed/unsupported reads or timeout use `paste-unavailable`. Retained AX references stay on the main thread; workers carry only immutable tokens. A repeated show while an Ibis native window is key preserves the external capture/bank, even while NSWorkspace still reports the external app active. The selection read adds up to 30 ms to opening, only for apps Ibis may paste into, and up to 60 ms to a paste (decision 0029). Bounded capture latency adds up to 50 ms to opening; native provider latency remains to be measured. When a permitted app leaves out document/surface attributes, Ibis automatically enables AXManualAccessibility once per live process, then repeats the full capture inside that same budget (decision 0029). The NSRunningApplication retained when eligibility was decided must still be the live instance for that PID before the write. Optional in-process diagnostics emit only fixed stages, numeric timing/errors, owner/equality booleans and whether capture attempted the activation write. A successful copy records the pasteboard change count, target, picker presentation and write time in a one-use proof. After the picker hides, bounded readiness checks wait up to 1,000 ms at 25 ms intervals for the original captured document window and surface, even if the app stayed active throughout; they never capture a replacement. Then the full final proof is repeated and one layout-aware ⌘V is posted only to that process. The final foreground, Secure Event Input, change-count and deadline checks run together with posting in one main-thread task. Secure Event Input is system-wide, may be enabled by another app, and does not identify the destination's field or detect every password control. An enabled flag copies with `secure-input`; unavailable checks copy with `paste-unavailable`; expired delivery uses `paste-expired`. Both notices offer ⌘V; neither claims the destination is a password field (decisions 0009, 0029). A replacement clipboard or changed picker also only copies. Paste reads no clipboard contents. Auto-paste needs both PostEvent and Accessibility; passive checks never prompt. Settings reports granted only when both are authorized and guides the person to the version-specific Accessibility list. Its explicit guide action repairs only missing grants, including Accessibility; passive checks never request either grant. Granting never revives a failed capture or retries delivery: return to the external document, summon Ibis again and activate the citation afresh. All ownership locks precede the final identity reads. After the last AX comparison, combined-session key-down and left/right/other mouse-down timings must show no input since delivery began; local lifecycle/time and deadline guards finish before posting. Mouse movement does not count. A programmatic tab or conversation change with no input is not detected. PID-directed posting cannot atomically bind to a foreign window: a small sequential focus/session/clipboard observation and focus-read-to-post gap remains (decision 0029). Terminals, remote desktops and VMs are also refused. Windows: the window in front is captured the same way (executable file name, process, integrity level), the picker shows above always-on-top windows, and the keyboard is handed back to that window. A one-use delivery ticket freezes the target, HWND and picker generation before copying, then records the final clipboard sequence after Ibis's own write. One atomic Ctrl+V `SendInput` goes in only while that window and process are in front, no key is held and that sequence is unchanged; apps with more rights than Ibis and UI Automation password fields copy instead. Password checks have a 250 ms caller deadline, no queue and at most one outstanding query per target process (four total); a stuck provider blocks only its own process. Display names are read off the UI thread with a bounded wait and executable-name fallback. Partial `SendInput` releases every key it left down, retries failed releases and retains them for recovery before any later paste. Linux copies and says why. Every refusal leaves a copied outcome. macOS keys PostEvent permission to the signing identity, so builds need a stable signing certificate.
- **Reader (0.5; trust boundary in [decision 0036](decisions/0036-reader-trust-boundary.md)):** PDF.js (`pdfjs-dist`, legacy build) behind a small adapter, loaded lazily. File bytes arrive over IPC from a command that checks the item and never exposes paths; PDF scripting and eval are off; a plain page-text view backs up accessibility. Ibis keeps the physical page index, the printed page label and an annotation's own page label apart. The printed label comes from the PDF's page labels, a Zotero annotation on that page, or an offset the user confirms; a confirmed offset covers a page range (front matter, missing pages and supplements break a single offset), is tied to the file's content hash and is dropped when the file changes. The record's page range is only a hint.
- **Accessibility:** combobox and listbox (not grid) for results, `TagGroup` for the shelf, one announcer, native announcements after the picker hides. Tested with axe in jsdom, Playwright WebKit snapshots, native accessibility trees and screen-reader runs per release.
- **Paste permission detour:** delivery returns its typed outcome to the picker.
  A `no-permission` or `accessibility-required` result keeps the prepared shelf
  and output in memory rather than consuming them. Settings rechecks both
  authorizations every second while its
  General page is visible and pasting is on, and immediately on return; older
  reads cannot replace newer ones. A refused-paste deep link closes after a
  native grant/focus check and queued settings saves; it never reopens the
  picker. The person returns to the external document and summons Ibis again.
  That presentation captures the destination and retires the old clipboard
  proof. No paste is attempted until a fresh Enter.
- **Language:** the engine returns message codes; the app words them through Fluent from 0.3, so German and one right-to-left language can follow in 0.4. Interface language, citation language and text direction are separate; mixed-direction names are isolated in the preview and on the clipboard.
- **Budgets (release builds, limits from 0.3):** the limits and sizes are in decision 0032; #1536 gives each its check and channel. No polling loops while hidden. Engine search and picker asset size fail pull request CI; open latency, idle CPU, wakeups and whole-app memory (web view processes included) are measured warm and cold, at least 30 samples after a 60 s settle, on the named release machines before each release. All use invented libraries, report p50/p95/p99 and compare against fixed limits rather than only against `main`. Superseded previews and searches are cancelled, not ignored. The heap an opened library holds is counted exactly at 10,000 realistic records by `crates/ibis-engine/tests/library_memory.rs`, which fails pull request CI. Name lists, most of that heap, are held as compact typed names inside `Record.csl` (`contract/csl.rs`) and serialize to exactly the JSON they came from, so fingerprints and every front end are unchanged; 10,000 records hold 44 MiB (from 86 MiB). Bulk readers (the search index, fingerprint checks, identifier scan, export) read names as held; asking for one list as JSON builds and keeps it for that record only.

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
Core retains the shared name/date helpers, settings, transport and
group-list rules still used by clients. The native shortcut, panel, clipboard and
updater modules, UI components, tokens and synthetic fixtures remain. Settings
migrations and identity carry-over code are deleted (decision 0027).
