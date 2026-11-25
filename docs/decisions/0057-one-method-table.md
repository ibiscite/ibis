# 0057 One method table for every surface

Decided 2026-11-18 for the structural series in
[#1997](https://github.com/ibiscite/ibis/issues/1997). It carries out stage 4 of
[0054](0054-owners-and-boundaries.md) and narrows that decision's "catalogues
are local declarations" choice: the table generates routing, schemas and
projections, never handlers. It does not change the wire rules of
[0028](0028-engine-first.md) or the open layer of
[0056](0056-open-layer-for-research-material.md).

## Why

- A public method is written in about seven places: the `Method` enum, its
  `ALL` list and names, the request schema match, the dispatcher, the result
  schema match (which falls back to an untyped schema for a method it forgets),
  the command line's options and its hand-built parameters.
- MCP is a second contract: eleven tools with hand-written JSON schemas,
  hand-parsed arguments, their own names (`omit_author`, `find_in`) and their
  own limits. Nothing notices when they drift from the typed requests.
- The app has 95 commands and one of them goes through `dispatch`. About 50 are
  engine or settings work that no other client can reach.

## The table

Each capability is one row, declared once in `ibis-engine` with a
`macro_rules!` table: no procedural macro and no new dependency.

| Field | Says | Generates |
| --- | --- | --- |
| `name` | The wire name | `Method`, `ALL`, names and parsing |
| `Params`, `Result` | Serde and schemars types | Both schemas, typed `Engine::call::<M>`, the dispatcher's routing, TypeScript |
| `handler` | The engine function, written by hand | Dispatch |
| `docs` | One catalogue line | Schema description, `--help`, MCP description, docs |
| `errors` | The error codes it can return | The schema's error list, CLI exit table, MCP tool errors |
| `tier` | `stable` or `app` | Which part of the schema the contract differ guards |
| `needs` | What an open library must offer: nothing, records or attachment text (`as-called` for `call`) | Which methods a front end offers, with consent added on top (0055) |
| `network` | `offline`, or the hosts it may reach (`as-called` for `call`) | PRIVACY.md, consent screens, the outbound client's allowlist |
| `palette` | The selections the ⌘K list offers it for, or none | The ⌘K list |
| `lane` | `read` or `work` | Which queue of the connection runtime runs it |
| `cli` | None, or how it is typed at a prompt | The clap subcommand |
| `mcp` | None, or its projection | `tools/list` and `tools/call` |
| `access` | Which app windows may call it | The Tauri command and its per-window grants |
| `needs` | The library capability it requires ([0059](0059-works-with-nothing-better-with-more.md)) | Whether it is offered for a library, and its refusal |
| `network` | None, or the hosts it may reach | Consent screens and PRIVACY.md's network list |
| `palette` | Whether it is a ⌘K verb, and for which selection kinds | The ⌘K list |

- `dispatch(name, json)` is parsing the name, deserializing `Params` and
  `call`. In-process callers use `call::<M>` with typed values and no
  serialization.
- No row has an untyped result; a test fails on an empty schema.
- `node scripts/gen.mjs` writes every derived file from the table. Each step
  below proves itself by leaving the generated fixtures unchanged, unless the
  pull request records the break in `fixtures/contract-breaks.json`.

**Command line.** Options are built from the `Params` schema: field docs become
help (where today's help reads better, the field doc takes its words, so steps
3 and 4 change descriptions and nothing else), enums become choices, required fields stay required, numeric bounds
become value checks. The `cli` field adds only what a schema cannot say:
positional fields, examples, presentation flags such as `--keys`, and the
human renderer. Parsed options become one JSON object that the same `Params`
type deserializes, so there is one parser. The library flags are one shared
choice flattened into each request. `records` stays `cite --records`, declared
on its row as an alias. Porcelain with no method (`doctor`, `completions`,
`serve`, `mcp`, agent setup) stays in the CLI crate.

**MCP.** A tool is a projection of a row, never a second parser: an `Args`
type that schemars describes and serde reads (unknown fields refused), one
conversion into `Params`, and a typed view of the result whose schema is the
tool's output schema. The projection also says when the tool is offered (text
tools only when a library allows attachment text; proposal tools only when
writes are on) and whether it reads or proposes, which sets its annotations.
Answer limits and the provenance sentence live in the view. Argument names
follow the contract's (`suppress_author`); the renames are one recorded break,
made before the MCP Registry listing. MCP becomes a row of the front-end parity
test.

**Tiers.** `stable` rows are the documented plumbing that 0028's rules and the
contract differ guard. `app` rows (settings, library management, sessions,
context, proposals) are in the schema, marked as app tier, callable from the
CLI and MCP, and free to change until 1.0. 0056's stable surface is the stable
tier, and its "no private door" becomes true because the app has no other way
in.

## One connection runtime

`serve` (JSON-RPC) and `mcp` share one runtime in `ibis-engine`: framing, the
id map, cancellation, bounded queues, and a busy reply the reader sends without
waiting on a full queue. Each protocol is a codec over it. The row's `lane`
picks one of two queues, so a slow `read` of a PDF no longer holds up `search`.
The per-user socket planned for 0.7 is one more transport.

## App commands, family by family

Tauri commands are generated per row (a `macro_rules!` expansion that calls
`engine.call::<M>`), and their per-window grants are generated from `access`.
Tauri's manifest still declares and grants every command per window, so
[0024](0024-window-permissions.md) holds unchanged. One generic `call` command
was rejected: it would move window permission out of Tauri's checked grants
into Ibis code.

Each family is one pull request, or two when it passes 1,000 changed lines with
the old path deleted in the second: add the rows at app tier, generate the
commands, point the interface at the generated bindings, and delete the hand
wrapper, its bindings entry and the CLI's hand-written file.

1. Settings and backups (about 13 commands).
2. Libraries (about 14).
3. Search, preview, session references and recents (about 11).
4. Context and projects (about 3).
5. Proposals, agent apps and attachment text (about 11).

Native effects (windows, paste, shortcut, updater; about 43 commands) stay
native and are declared later in the same form.

## Three primitives

| Primitive | Lives in | Shape | Makes impossible |
| --- | --- | --- | --- |
| `Epoch` | `ibis-core` (pure), one `latest()` in the interface | Each owner holds an epoch; `begin()` returns a stamp; state changes only through `stamp.commit(…)`, which does nothing once a newer stamp has begun. On the wire it is one opaque `revision` string on every observation | Work begun against an older library, session, selection or clipboard changing newer state. It replaces about a dozen hand-built generation guards in Rust and the counters in about 22 interface files; a test refuses a new bare counter outside the module |
| `Delivery` | The native delivery owner: `ibis-native` once it lands, driven by `delivery.rs` | 0054 stage 9 already made Enter one `deliver_citation` with one typed outcome. Left: one ticket type on every system (today only Windows has one), one flat outcome, the clipboard write private to the module with a stamp as the only "nothing newer was written" guard, and the bibliography and project-entry copies as artifacts of the same delivery | A second clipboard writer, a late copy replacing a newer one, an outcome without a receipt, a fix on one copy path that misses another |
| `LibraryState` | The engine's library owner, published as the libraries row's result | `None`, `Opening { requested }`, `Ready { revision, count, checking, retained }`, `Failed { requested, problem, kept }`, replacing the flat status record | A failure beside a loaded library, one failure said in several fields, an interface rebuilding a reason from fields: the words come from `Problem` only |

## How others derive their surfaces

| Project | Pattern | What Ibis takes |
| --- | --- | --- |
| Zed | Actions are declared once as named, typed values; the command palette and key bindings read that registry, and handlers are attached separately. | Declarations are data; handlers stay code. |
| SQLite | The parser is generated from one grammar, and the command-line shell is built on the public API alone. | One source for the parser; the CLI has no private door. |
| LSP servers | Each request is a type with a method name, parameters and a result; a dispatcher routes by name to a typed handler; the protocol is published as a machine-readable model. | A row is name, parameters and result; dispatch is routing. |
| MCP servers | A tool's input schema is derived from the type its handler reads. | MCP arguments are a projection of the typed request. |

## The series

Before it: #1981, #1982, #1983 and #1985 merged first. #1984 (DOI lookup)
and #1991 (`ibis format`) no longer gate step 1 (amended 2026-11-25): they
re-land later as rows of the table, keeping their engine modules. #1987's
differ is welcome but not required: each step's proof is its unchanged
fixtures. No new feature lane adds a hand match once step 1 lands; new
methods arrive as rows. Each step lands alone on a
green main; the old path goes in the last step of its family.

1. **The table and the catalogue family.** `styles`, `outputs` and `schema`
   become rows in `method.rs`: they read no library and take small requests.
   The method list, names and both schema functions read the table, and
   dispatch routes these three through `call`; the other twelve keep their
   match arms. Step 1 derives only the per-method schemas, so its proof is
   `node scripts/gen.mjs --check` with `fixtures/cli-schema.json` unchanged,
   plus a test that the rows name their methods. CLI options, MCP tools and
   bindings follow in steps 3, 5 and 9.
2. **Every engine method.** The other twelve rows; delete the hand `ALL`, the
   name match, both schema matches, the dispatch match and the untyped
   fallback. `schema` and `call` still declare an untyped result here, so the
   fixture stays unchanged; one small pull request after this step types
   both, adds the test that every schema is non-empty, and lists the two
   changed entries of `fixtures/cli-schema.json`.
   Then, before step 3 (added 2026-11-25 from the #2035 replan): the rows
   gain `needs`, `network` and `palette`, published in `schema`'s answer.
3. **CLI, catalogue and library reads** derived from the table.
4. **CLI, the rest**; delete the hand subcommands and hand-built parameters.
5. **MCP, search, show, styles and cite** as projections; parity gains an MCP
   row.
6. **MCP, reading and proposal tools**; delete the hand schemas and argument
   parsers; record the argument renames once.
7. **One runtime under `serve`**, with the two lanes.
8. **`mcp` on the same runtime**; delete the second framing, queue and id map.
9. **App family 1, settings.** It introduces the generated Tauri commands and
   grants, so it touches app permissions and needs an independent review.
10. **App family 2, libraries**, with `LibraryState`.
11. **App family 3, search and sessions**, with `revision` on the wire.
12. **App families 4 and 5**, one pull request each.

`Epoch` and `Delivery` run beside steps 3 to 8 once their files are free:

- `Epoch` in `ibis-core` with a property test over interleavings; then the
  engine owners; then the desktop guards; then `latest()` in the interface,
  deleting the counters and adding the test that refuses new ones.
- `Delivery` after the `ibis-native` crate lands: one ticket type on every
  system; then the flat outcome with the list copies moved in, deleting the
  second guard and the raw clipboard writes. Paste is a security boundary
  ([0029](0029-native-calls.md)), so each needs an independent review.

## Not doing

A procedural-macro framework, generated handlers, a generic app `call`
command, and any compatibility shim or old name kept for older clients before
1.0. Steps 1 to 4, 7 and 8 change nothing on the wire.
