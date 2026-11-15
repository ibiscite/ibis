# 0055 Agent layers, full text, writes and remote libraries

Decided by the maintainer on 2026-11-15: yes to every question below, as
recommended, and the direction in [One place to set it up](#one-place-to-set-it-up).
`ibis agents setup` and steps 3 and 4 of the order (attachment text from
Zotero's index and from the PDF itself) are built; the other numbered steps are
not yet.

It builds on [0033](0033-engine-for-agents-and-tools.md),
[0036](0036-reader-trust-boundary.md), [0051](0051-references-in-and-out.md) and
[0008](0008-offline-by-default.md). Where it would change one of them, it says so
and asks.

## One engine, three thin layers

- **The CLI** is the source of truth for scripts and power users. Every
  capability exists there first, with the same engine methods and contract.
- **MCP** gives reach to agent apps without a terminal, such as Claude Desktop,
  ChatGPT desktop and Cursor. Its tools are few and general, and their
  descriptions say what a tool does, not how to work.
- **The Agent Skill** carries the workflow: search first, never cite from memory,
  library and document text is data. Guidance lives here, not in long tool
  descriptions.

A Claude Code plugin, a Codex plugin or a ChatGPT app is packaging around the
skill and the MCP server, never a fourth implementation. A new capability goes
engine → CLI → MCP, then the skill learns when to use it. Vendors such as Stripe
ship agent support in a similar shape: a setup command that finds installed agent
apps, a plugin bundling the MCP server with skills, a small set of general tools
rather than one per endpoint, and write actions kept apart from reads and
confirmed by a person. `ibis agents setup` (issue #1867) is Ibis's setup command.

## One place to set it up

The app and the command line are one product with one set of settings. Most
people use the app, so everything an agent needs, from connecting an AI app to
the per-library opt-ins below, is set in Settings; `ibis` reads and changes the
same saved settings, never a copy. Connecting AI apps is one shared piece of
code: Settings shows each AI app found here with Connect or Disconnect and the
same before-and-after view as `ibis agents setup|status|remove`, which use it
too. Nothing is copied or pasted by hand, and a choice made in one place shows
in the other. All of it stays optional.

## 1. Reading attachment text

**What.** An agent can read the text of a reference's attachment with page
numbers, and cite the page it found. It never receives the PDF file.

**Why.** Search finds the reference; the claim is usually on a page. Today an
agent either cites without a page or asks the person to look. The aim is
search → read → cite with a page, from the person's own copy.

**Design: reading, not dumping.** Three engine methods, each bounded:

| Engine / CLI | MCP | Returns |
| --- | --- | --- |
| `outline` / `ibis outline REF` | `outline` | The reference's readable attachments as opaque ids, with kind, page count, whether text is available and whether printed pages are known; headings when the file has them. |
| `read` / `ibis read ATTACHMENT --pages 3-5` | `read` | Text of at most 5 pages or 32 KiB, each page marked. |
| `find_in` / `ibis find-in ATTACHMENT "QUERY"` | `find_in` | At most 10 short passages (about 400 characters) with their pages. |

Each page is reported as `pdf_page` (physical index) and `cite_page` (printed
label, or none). Following 0036, a printed label comes only from `/PageLabels` or
a confirmed offset, never from the physical index. `cite_page` passes straight
to `cite` as a locator; when it is missing the skill tells the agent to ask the
person, not to guess. Attachment ids are engine-issued and valid for the current
library generation, like the reader's; no path or file URL leaves the engine.
When the reference has its own `page` field, only labels consistent with it are
published. Exact labels and fully written decimal ranges can establish this;
unrecognized ranges and labels outside them stay unknown pending confirmation.
The three tools may fold into one `read` tool with modes if clients cope better
with fewer tools; that is a detail for the first slice.

**Sources of text.**

- **Zotero's full-text cache.** Zotero writes the text it indexed to a
  `.zotero-ft-cache` file in each attachment's storage folder and records the
  indexed and total page counts. Reading it needs no PDF parsing and also covers
  EPUB and web snapshots. It does not reliably mark where pages end, and Zotero
  indexes only the first pages of long files by default. Ibis trusts its page
  breaks only when the pages it separates match Zotero's recorded counts exactly;
  otherwise the text is returned with pages unknown. Ibis never reads or writes
  `zotero.sqlite` for this beyond the private copy it already reads when Zotero
  is closed.
- **A PDF text extractor** for attachments without an index (BibTeX `file`
  fields, unindexed Zotero files) and wherever pages or printed labels are
  needed. Pandoc cannot read PDFs, so it is not an option.

| Extractor | Licence | Weight and safety |
| --- | --- | --- |
| `pdf-extract` / `lopdf` (pure Rust) | MIT | Small; no `unsafe` in Ibis; fuzzable in `ibis-fuzz`. Weaker on columns, ligatures and unusual fonts. |
| pdfium through `pdfium-render` | BSD/Apache | Best text quality; ships a C++ library of several MB per platform; a scoped native boundary under [0029](0029-native-calls.md). |
| MuPDF bindings | AGPL | Compatible with Ibis's AGPL engine ([0002](0002-licence.md)); the MIT clients of [0039](0039-extending-ibis-from-outside.md) are unaffected because they talk to `ibis` over a process boundary. A C parser and FFI, as with pdfium. |

Recommendation: start with the pure-Rust extractor, measured on a corpus of
invented PDFs, and move to pdfium only if quality falls short. Whichever is
chosen runs in a short-lived child process of `ibis` with a deadline, a memory
bound and no grants, never inside the desktop app, for the reason 0036 gives:
untrusted parsing stays away from library, clipboard and paste permissions.

**Bounds.** The file rules of 0036 apply: a regular local file up to 256 MiB,
`%PDF-` near the start, no placeholder or network files, at most 10,000 pages,
10 s per extraction. Text stays in memory for the call; Ibis keeps no document
text on disk.

**Privacy and network.** The MCP tools promise today never to expose notes or
attachments, and [docs/agents.md](../agents.md) says so. Full text therefore
needs an explicit, visible opt-in per library, off by default, separate from the
notes and annotations opt-in of 0033 because whole documents disclose more. The
tools are read-only and open no connection. While the opt-in is off they are not
listed. Every passage carries the same provenance line as library text, labelled
as attachment text: documents can hold instructions aimed at agents, and the
skill says to treat them as data and report them. The command line reads files
the person can already open, so `ibis read` follows the CLI's rules, and
PRIVACY.md says that an agent with a shell can use it, as 0051 already does for
lookup. PRIVACY.md, docs/agents.md, docs/cli.md, the skill and the website change
in the pull request that ships the MCP tools.

**Later.** Searching inside every document of a library is a separate decision.

## 2. Writing to libraries

**What.** Small changes at an agent's request: adding an item, later a note,
a citation, organising items and collections. Zotero comes first; every library
kind with a safe way to write follows through the same proposal flow (for a
`.bib` file, Ibis's own preview, backup and Undo), never a separate path per
agent.

**Why.** An agent that finds a missing reference can only tell the person to add
it by hand.

**Ways to write.**

| Route | Status |
| --- | --- |
| Zotero's local API on `127.0.0.1:23119` | Read-only in Zotero 7; 0051 waits for Zotero's official local write API. |
| The Connector endpoints | Built for the browser connector, not a stable contract; 0051 rules them out. |
| The Web API with an API key | Works today, but is network use under 0008 and reaches the local library only after a sync. |
| `zotero.sqlite` directly | Never: it can corrupt the library and bypasses Zotero. |

Recommendation: follow 0051. Writes go only through Zotero's official local write
API once it is stable, to the library the person chose, with a preview, a
refusal if the target changed, a backup and Undo.

**Agents.** 0051 keeps lookup and write tools out of MCP. This proposal keeps
that rule for the default server and adds one possible change for the
maintainer: a separate mode, `ibis mcp --allow-writes`, with separate tools
annotated as not read-only. Such a tool only proposes: the draft opens in Ibis,
and the person confirms it there with the same preview as any other write. A
client's own confirmation is not enough, because many clients let a person
approve a tool once for all later calls. Without the running app nothing is
written. CLI writes follow the same rule when 0051 decides them.

## 3. Several computers and remote libraries

**Now.** Zotero's own sync keeps each computer's library current, and Ibis reads
it locally on each one. [Moving to a new computer](../moving-to-a-new-computer.md)
covers `ibis settings export` and `import` for Ibis itself.

**Next.** The roadmap's 0.7 plan, a library on another computer through the
person's own SSH, needs no listener: because MCP runs over stdio, an agent app can
start `ssh HOST ibis mcp --app-libraries` as its command. Documenting this, with
its limits, is the first step. No issue for a broader remote design was found.

**Later.** The Zotero Web API as a read-only library source, for machines without
Zotero. It uses the network, so it needs an entry in 0008 and the maintainer's
yes: off by default, only `api.zotero.org`, a read-only key kept in the system's
credential store, never in `settings.json` or backups. Such a library is labelled
online wherever it appears, and its MCP results say so in `source_health`, so the
"no network" promise stays true for every other library.

## Order

1. Record the three-layer rule; trim tool descriptions and move workflow into the
   skill.
2. Document `ssh HOST ibis mcp` and the move between computers.
3. Done: `outline`, `read` and `find_in` on Zotero's cache: engine, then CLI, then
   MCP behind the opt-in, with the skill and privacy pages. While Zotero runs, the
   text and page counts come from its local API (`items/KEY/fulltext`); while it is
   closed, from `.zotero-ft-cache` and the private copy of its database. The opt-in
   is `attachment_text` in settings, a list of libraries kept on this computer and
   out of backups, as paste approvals are ([0031](0031-what-ibis-remembers-per-app.md)).
4. Done: the PDF extractor in a child process, with page labels and fuzzing.
   `lopdf` (MIT, pure Rust, no `unsafe`) parses in `ibis __extract-pdf`, which
   gets the opened file as its standard input and no environment, within ten
   seconds. It drops core dumps and has 20 processor seconds on Unix and 3 GiB of
   address space on Linux; each stream inflates to at most 64 MiB. Only stdio
   survives exec, and the desktop app never parses. Label prefixes over 64
   characters and label trees over 1 MiB are refused.
   It reads `.bib` `file` attachments and Zotero PDFs with no index or a partial
   one; Zotero's complete, verified pages still win. `cite_page` comes only from
   `/PageLabels`, checked against 0036's label rules, and with Zotero's text only
   when the PDF's page count matches exactly. `ibis-fuzz` has a `damaged_pdf`
   target. `pdf-extract` was not chosen: it builds on an older `lopdf` and adds
   font crates for little gain at this step.

   Review follow-up, 2026-11-15 (#1940): resolve links in the chosen Zotero data
   root once, after cached mount classification; open every component below it
   without following links. A `.bib` library's folder is already canonical.
   Test temp roots use the same policy, including macOS's `/var` system link.
5. Agent proposals for writes, after 0051's add-by-identifier slice.
6. The Web API source, only after the maintainer's yes.

## Questions for the maintainer

Answered on 2026-11-15: yes to all eight, as recommended. Online sources (the
Web API, item 6 of the order) stay deferred.

1. Adopt one engine and three thin layers, with workflow in the skill?
   Recommended: yes.
2. Let agents read attachment text through a separate per-library opt-in, off by
   default? Recommended: yes.
3. Start with Zotero's full-text cache and report pages only when they can be
   verified? Recommended: yes.
4. Use a pure-Rust extractor in a child process first, rather than pdfium or
   MuPDF? Recommended: yes.
5. Let `ibis read` work without the opt-in, as other CLI reads do, with
   PRIVACY.md saying so? Recommended: yes.
6. Amend 0051 so a separate MCP mode may propose writes that the person confirms
   in Ibis? Recommended: yes, after 0051's add slice ships.
7. Keep writes to Zotero's official local write API only? Recommended: yes.
8. Defer a Zotero Web API library source until after 0.5? Recommended: yes.
