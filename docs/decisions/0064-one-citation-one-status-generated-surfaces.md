# 0064 One citation, one status, generated surfaces

**Decided** 2026-11-26, from an architecture review of every layer (engine,
desktop, CLI, `serve`, MCP, editor endpoint and plugins) at main `463bc16c`.
The maintainer chose two product points; the engineering choices follow from
them and from [0057](0057-one-method-table.md),
[0062](0062-closed-parts-owned-space-fitted-layers.md) and
[0063](0063-form-follows-function.md).

## The maintainer's answers

- **One citation everywhere.** A citation started in the command line, an
  editor or an `ibis://` link can continue in the picker, and the other way
  round.
- **Contracts stay changeable until 1.0.** Ibis is still finding its shape:
  - the typed-citation syntax is one engine grammar shared by every front
    end, documented as unstable until 1.0;
  - settings may still reset on an update before 1.0, as
    [0016](0016-no-compatibility-before-1-0.md) says;
  - a better architecture is preferred over adding to a weaker one.

## Why

| Problem today | Class of bug it causes |
| --- | --- |
| A library failure travels in seven fields (`library/status.rs`), and the interface rebuilds the wording and the fix (`libraryWords.ts`) | Each front end says a different thing, and a fix reaches one place and misses another |
| Refusals cross IPC as English sentences (`WriteError::sentence()`, `ProposalResult::Refused{sentence}`, notice `text`) | German and Arabic show English that names a command-line flag |
| The picker adds its own format precedence over the engine's rule (`destination.ts`) | Late-save races; the picker and the editor or CLI disagree on the format |
| Event names, payloads, clap options, MCP schemas, command and permission lists are kept by hand | Mirrors drift (for example `omit_author` against `suppress_author`) |
| App work such as proposals, agents, backups and projects is wrapped twice, once for the CLI and once for the desktop | Error mapping differs by front end |

## The decision

1. **Words only from the catalogue.** No IPC or engine type carries a
   human-readable `String` message. Every refusal is a `Problem` with a code,
   its subject and a recovery, and each front end words it from `locales/`. A
   ratchet refuses new string message fields. Problem codes use one case.
2. **One status owner.** The library, settings, delivery and the updater
   report to it. It publishes one typed list of `Problem`s with a revision.
   Search replies, the empty list, the tray and `ibis doctor` all read that
   list. It is the source of 0063's one status line.
3. **The engine holds the citation being built.** Each citation session
   publishes a **Draft**: the parsed citation text, its tokens, the context
   (including "this paste") and a revision. Any front end sends intents by
   session id through the method table and holds no handle. The web view keeps
   focus, the drawer and scrolling. The format the engine reports is the
   format; the picker has no precedence of its own.
4. **Surfaces are generated.**
   - **Tauri commands, CLI options and MCP tools** come from the method table,
     as 0057 plans.
   - **Events:** a topics table generates event names, payload types and the
     TypeScript listeners.
   - **Settings:** one settings row with a typed patch and a published JSON
     Schema. The `ibis` command, editors completing `settings.json`, and
     Settings' rows as data ([0062](0062-closed-parts-owned-space-fitted-layers.md))
     all read it.
   - **Native rows** declare their `access`, which generates capabilities and
     grants.

## Order

Each step lands as a new primitive beside the old path; callers move; then the
old path is deleted.

1. Words only from the catalogue.
2. 0057 steps 3 to 6: CLI options and MCP tools from the table, so MCP joins
   the parity test.
3. The status owner. This is 0063's status line; the flat status fields and
   `libraryWords.ts`' reasoning are deleted.
4. 0062 steps 1 to 3: closed parts, regions, and the gate. These are front-end
   only and can run beside the steps above.
5. 0063's empty-search history.
6. 0063's formats in the picker only. "This paste" and the held format become
   context facts; Settings › Citations is deleted before any generator would
   emit its commands.
7. The topics table; then the hand-kept event names are deleted.
8. The settings row and its schema, with 0062's settings recipe rendering
   from it.
9. The citation-text grammar in `ibis-core`, with shared fixtures. The CLI and
   MCP use it first.
10. 0057's remaining families with `revision` on the wire, and the Draft. The
    reducer counters are deleted, then 0063's typed citations come to the
    picker.
11. The connection runtime. CAYW and the same-user socket become codecs over
    it, with an observation stream for outside clients.
12. Native rows with `access`, generating capabilities and grants. This
    touches app permissions, so it gets an independent review.
