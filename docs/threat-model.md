# Threat model

What could harm a person through Ibis, what stops it, and which check shows
that it does. It covers the 1.0 surfaces: library input, output, local
endpoints, updates and extensions. Report a gap through [SECURITY.md](../SECURITY.md).

## What is protected

- The person's library, documents and clipboard: Ibis reads libraries and
  writes only a citation where the person asked, or a file they chose.
- Their computer: no input may run code, read files outside what the person
  chose, hang the app or exhaust its memory.
- Trust in the citation itself: what is pasted is what the picker showed, from
  the records it named (prepared citations, [architecture](architecture.md)).
- Their privacy: no network use beyond update checks and opt-in lookups, and no
  library text in logs, diagnostics or crash output.

## Who might attack

| Attacker | Reach | Not in scope |
| --- | --- | --- |
| Author of a shared library file, PDF or EndNote export | Bytes in a file the person opens | Convincing the person to open it |
| Another program on the same computer | Answering on Zotero's port, writing Zotero's files, connecting to the warm engine's socket from another user's session, naming itself as an adapter only if the person configures it | A program running as the person can already do what they can |
| Text inside a record, aimed at an AI agent using `ibis mcp` | What the agent reads back | The agent's own judgement |
| Someone on the network | Update downloads and the update feed | — |
| Someone who controls the build or release | Packages people download | The maintainer's own accounts |

## Malicious library input

Every reader takes bytes it did not write. Each must refuse or report a
warning, never panic, never exceed its bounds, and publish only records that
validate. File libraries are capped at 64 MiB and 200,000 records, and each
load has a deadline and a cancellation token. One record is capped at 1 MiB of
CSL data in every reader that builds CSL data directly (CSL-JSON, caller and
adapter records, RIS and CSV; `MAX_RECORD_BYTES`, tested at the boundary in
`crates/ibis-sources/tests/exported_libraries.rs`); BibTeX instead bounds each
value at 64 KiB and the whole load by a work budget (`bibtex/resolve.rs`,
`bibtex/mod.rs`). The fuzz targets use inputs up to 64 KiB (PDF: 1 MiB), so
they exercise parsing, not these size limits, which the tests cover.

| Input | Reader | Fuzz target (`fuzz/`) |
| --- | --- | --- |
| BibTeX / BibLaTeX file | `ibis_sources::load_file` | `damaged_bibtex` |
| CSL-JSON file; records a caller sends over JSON-RPC or MCP | the CSL-JSON reader (`load_file`, `request_records`) | `damaged_csl_json` |
| RIS file | `load_file` | `damaged_ris` |
| CSV file | `load_file` | `damaged_csv` |
| PDF attachment (page labels, text) | `ibis __extract-pdf`, in a child process with a deadline | `damaged_pdf` |
| An adapter's answer | `SourceAdapter::answer` | `adapter_answer` |
| EndNote XML export | the built-in EndNote adapter, then its parent | `endnote_export` |
| Zotero local API answers | `zotero_local::live::LiveReader`, then `zotero::map_item` | `zotero_live` |
| Better BibTeX's `read-only.json` | `zotero_local::bbt::read_only_keys` | `bbt_keys` |
| A `.bib` file Ibis edits in place (decision 0055) | `bibtex_edit::BibFile` | `bibtex_edit` |
| A project's bibliography, read to check keys | `project::ProjectRead` | `project_bib` |
| Zotero's `profiles.ini` and `prefs.js` | `zotero_local::discovery::discover` | `zotero_profiles` |
| An editor endpoint request (opt-in, loopback) | `ibis_engine::cayw::parse` | Not yet: unit tests only (see below) |
| An open EndNote library (macOS scripting) | the same XML reader as an export | `endnote_export` |
| Zotero's full-text cache | `zotero_local::fulltext::pages` splits text on page breaks; nothing is parsed | — |
| Zotero's closed database and Better BibTeX's store | SQLite on a private copy, opened read-only | Not yet: SQLite itself is fuzzed upstream (OSS-Fuzz), and the rows reach the same `map_item` as `zotero_live`, but the reading of rows is not fuzzed here |

Attachment paths in a library are data: they are probed in one bounded batch
per load, a caller's CSL-JSON `file` field is dropped unread, and
cloud placeholders are refused before reading. CSL styles and locales are bundled, pinned by hash in
`crates/ibis-csl/vendor/manifest.json`, and never read from a library or a
caller, so they are not an input.

Each oracle also checks what a reader publishes: invalid UTF-8 is never
accepted, every record survives its wire form unchanged and carries a CSL
type, keys come only from the format that may give them (decision 0043), a
snapshot rebuilds to the same fingerprints, a contained panic still fails, and
any input takes under five seconds.

## Output injection

A record's text is data in every output. Each citation syntax escapes what it
writes (`escaping_properties_for_every_current_syntax` and
`control_characters_are_refused_in_every_affix_target` in
`crates/ibis-core/tests/syntax_contract.rs`). Terminal output cannot carry
control sequences or extra lines (`untrusted_controls_cannot_execute_in_terminal_output_and_json_stays_literal`,
`human_result_rows_cannot_inject_extra_lines_or_terminal_controls` in
`crates/ibis-cli/tests/`). Links in shared packs come only from DOIs and public
URLs, and the clipboard receives only the checked citation, re-cited and
compared with the shown hash at commit ([architecture](architecture.md)).

## Local endpoints

`ibis serve --stdio` and `ibis mcp` speak over stdio to the program that
started them. MCP tools are read-only; their results mark library text as data
so an agent does not act on it.

The **warm engine** (`ibis daemon`, [decision 0065](decisions/0065-one-warm-engine-thin-shells.md))
listens on a same-user local socket and never on the network. The CLI and
each `ibis mcp` session connect to it when it is available and start it on
demand; without it every command runs in its own process. On Linux
and macOS the socket sits in a directory of mode `0700` that the person owns,
and each end checks the other's user ID (`SO_PEERCRED`, `getpeereid`). On
Windows it is a named pipe created as the first instance, refusing remote
clients, with a DACL for the person's SID and a token check on each end. Every
connection starts with a handshake naming the protocol and build; at most 32
connections are served at once. It answers the same method table as
`ibis serve --stdio` and the requests the `ibis` command itself makes, so it
can do nothing a program running the `ibis` command as the person could not. Another user's session cannot reach it. Tests:
`daemon/unix.rs` and `daemon/windows.rs` (another user's peer, a shared socket
directory, a second first instance) and `crates/ibis-cli/tests/daemon.rs`
(capacity, crashes, build changes, the same answers as stdio).

**Paste approvals** (`apps.json`, `ibis_engine::grants`) record the person's
own answer per app, and only Ibis's own picker writes them. They are consent,
not a security boundary: any program running as the person can edit the file.
What they stop is a paste nobody approved, including one an agent's text asks
for: no MCP tool, record or editor request can grant one. An unreadable file,
or one missing after it was saved, grants nothing until each app is answered
again. A paste waiting for its app to come back reads the app's choice again
before it queues the post. Tests: `grants/tests.rs` and, for the wait,
`a_choice_changed_while_the_app_comes_back_stops_the_paste` in
`crates/ibis-native/src/paste/tests.rs`.

**One paste per Return.** A shell names each Return with a random
`DeliveryId`. Before any engine grants the paste, it records the ID in
`deliveries.json` under a lock every Ibis process shares. A request carrying
an ID already recorded, from the same or another engine, can only copy.
Expiry uses the wall clock: an ID is forgotten after an hour or 64 newer
Returns. An
unreadable file lets nothing paste until it is replaced. Like the approvals it
guards against mistakes, not against a program running as the person. Tests:
`delivered/tests.rs` and, across two engines, `crates/ibis-cli/tests/daemon.rs`.

The **editor endpoint** (decision 0008) is the one TCP socket: off
until switched on in Settings, it binds a TCP port on 127.0.0.1 only
(`apps/desktop/src-tauri/src/editor_endpoint.rs`) so an editor can ask for a
citation, as Better BibTeX's Cite as You Write does. Any program on the
computer, or a web page through the browser, can send it bytes, so
`ibis_engine::cayw::parse` treats every request head as untrusted: a random
token in the path, new for each address; requests carrying headers browsers
add are refused, `Host` must name this computer (against DNS rebinding), and
answers carry no CORS headers; only `GET`; a head of at most 8 KiB within five
seconds; eight connections at once and one waiting request; fixed refusal
text that repeats nothing sent. Tests: the `cayw.rs` unit tests (browser
headers, host names, methods, oversized or broken heads) and the
`editor_endpoint.rs` tests (probe, pick, a second request, silent or oversized
clients, the port freed when switched off, a new token per address). Not yet
fuzzed: `cayw::parse` lives in `ibis-engine`, which the fuzz crate may not
depend on (`engine_boundary.rs`); a target needs that layer allowed first.

Ibis connects to Zotero's local API and Better BibTeX on 127.0.0.1 only. Any
local program could answer there, so those answers are untrusted input
(`zotero_live`, bounded requests, bytes, items and time) and refusals keep the
last good snapshot.

## Updates

Update packages are signed with a key built into the app, separate for Ibis and
Ibis Nightly, and checks read an HTTPS feed from GitHub
([decision 0019](decisions/0019-updates.md)). Publishing refuses a feed signed
by any other key (`scripts/update-feed.test.mjs`), and release packages carry
build provenance attestations, verified before publishing and verifiable by
anyone ([RELEASING.md](../RELEASING.md), which also covers the keys). The
daily installed-update qualification (`scripts/update-e2e.py`) updates one
nightly to the next.

## Extensions

A source adapter is a program the person named in settings, or the built-in
EndNote reader; Ibis never finds or installs one by itself (decision 0056). It
runs with the person's rights, so naming it is trusting it. What Ibis limits is
its answer: one request on stdin, a deadline, a 64 MiB answer, a closed
protocol and the same record checks as a file (`adapter_answer`). Agent skills
and the MCP plugin in `plugins/` only describe how to call `ibis`; they add no
code to the app.

## How it is checked

`cargo test -p ibis-fuzz` runs every target's seed corpus. Every night CI runs
it, then fuzzes each target for two minutes, keeping the
grown corpus in its cache between nights; a crash or a slow input fails the run,
keeps the input as an artifact and updates the nightly qualification issue. A
new reader is added to the table above and to `ibis_fuzz::TARGETS` in the same
change; the seed test fails for a target without its file or seeds.
