# Command line reference

`ibis` searches libraries and prepares citations without the desktop app.
`ibis --help` and `ibis COMMAND --help` describe the installed version.

## Install

The desktop app includes the matching `ibis` built from the same commit.

- **macOS and Linux deb/rpm:** the first time the installed app opens, it links
  `~/.local/bin/ibis` to the command inside its bundle or package, so where
  that folder is on PATH, installing the app is all it takes. It skips this when another `ibis`
  (Homebrew, a download, the other Ibis channel) comes first on PATH and while
  the app still runs from its disk image. After that, Settings › AI Apps › Command line
  decides: **Install the ibis command** adds it and removing it there is
  respected at every later launch. The link follows the app's updates.
  Keep the macOS app in its permanent location. Install creates the link even
  when the app's PATH omits it. If that directory is missing from the app's
  PATH, copy this line into your shell profile, then reopen your terminal:
  `export PATH="$HOME/.local/bin:$PATH"`. Ibis never requests administrator
  access or changes your profile. Linux packages install no command on PATH
  as root. On macOS, Finder's PATH can differ from your terminal; Homebrew's
  standard locations are also checked for conflicts.
- **Windows:** the per-user NSIS installer adds the app folder to your user
  PATH without elevation. Open a new terminal after installation. Channel
  installs serialize their PATH updates and refuse additions that exceed
  Windows's expanded PATH limit. Successful uninstall removes the installer's
  entry; cancellation preserves it.
- **Linux AppImage:** the first launch and the same Settings action install the
  command as an owned copy in `~/.local/bin`, so it keeps working after the
  image unmounts. It shows the same profile line when that directory is missing
  from PATH. When the same AppImage file is updated, its next launch updates
  the copy; for an AppImage saved under a new name, install again in Settings.

Settings shows the command's location, version and whether it matches this app.
Another command (including Homebrew) is never overwritten or run to inspect
its version; its location is shown with version unavailable. Unix PATH lookup
skips files without executable access. **Remove Command**
removes only an unchanged Ibis-owned installation and its receipt. Removing the
app itself on macOS cannot run cleanup: remove the command in Settings first.
A separately installed `ibis` (Homebrew, winget, Nix or a download) keeps its
own updates and is never replaced. Both read the libraries chosen in the app.
Then `ibis agents setup` connects your AI apps; see [AI agents](#ai-agents-mcp).

Standalone downloads, Homebrew and winget remain available. Download the CLI
archive from a [release](https://github.com/ibiscite/ibis/releases/latest) or
[Nightly](https://github.com/ibiscite/ibis/releases/tag/nightly):
`ibis-macOS-Apple-silicon.tar.gz`, `ibis-macOS-Intel.tar.gz`,
`ibis-Windows.zip` or `ibis-Linux.tar.gz`. Unpack it onto PATH. From a checkout:
`cargo run -p ibis -- --help`.

With Nix on Linux or macOS (x86_64 and aarch64),
`nix profile add github:ibiscite/ibis` installs the CLI and shell completions.
See [Nix installation](nix.md) for version pins and declarative configuration.

The standalone macOS CLI is unsigned; the bundled command follows the app's
signing. If a trusted standalone download is quarantined,
`xattr -d com.apple.quarantine ibis` removes that attribute.
Shell completions and an fzf recipe are in the
[terminal kit](../crates/ibis-cli/README.md).

## Libraries

| Option | Reads |
| --- | --- |
| `--file PATH` | A [supported file](compatibility.md#library-files); repeat for several. |
| `--zotero` | Local Zotero personal library, open or closed. |
| `--zotero-group ID` | A group library; repeat for several. |
| `--zotero-profile NAME` | A named closed profile when there are several. |
| `--source NAME[=LIBRARY]` | A library read by a [source adapter](source-adapters.md), such as `endnote=Invented.enl`; repeat for several. |
| `--app-libraries` | Libraries chosen in the desktop app. |

Library options before or after the command combine in order; duplicates are ignored. One-shot reads install no watches
and never change library files. Library IDs are opaque and repeatable on one
computer; [architecture](architecture.md#data-and-identity) explains identity.
Library commands save only the private identity key; explicit `settings` and
`projects` commands also write the settings you choose.

In `libraries --json`, each library lists the `capabilities` its reader
declares (`bibliographic-records`, key kinds, `attachment-text`, `proposals`, `original-entries`)
and a `revision` that changes whenever its published content may have changed.
A tool that mirrors a library compares `revision` and rereads only when it
differs. RIS and CSV files declare records only: they have no citation keys,
and neither do EndNote libraries. `ibis sources check` rereads the libraries
named with it and checks what every source promises; see
[source adapters](source-adapters.md#the-conformance-check). `ibis sources keep
--file FOLDER` keeps a folder of PDFs as a library in its `ibis-references.bib`
([compatibility](compatibility.md#a-folder-of-pdfs)); it is the only command
that writes a library file without a proposal, and only that one file.

Without explicit libraries, `search`, `show`, `cite`, `libraries` and `call`
of the first three use the app's choice. Stable settings take precedence;
Nightly is used when only it has settings. Missing, damaged, newer or unusable
settings fail without backup or other-channel fallback. `serve` and `mcp`
use app libraries only with `--app-libraries`.

## Commands

```sh
ibis search lantern --file refs.bib --limit 5
ibis cite vale2024lanterns --file refs.bib --style apa
ibis cite vale2024lanterns --locator "p. 12" fen2023clouds --omit-author --file refs.bib
ibis cite --request --file refs.bib < request.json
ibis outputs
ibis schema cite
```

Options after a key apply to that key. `--format` uses an ID from `outputs`;
`--style` selects any bundled CSL style. Bibliography entries use
`--bibliography` with a style. With no command at a terminal, Ibis prints a
start guide; without a command in a pipe, it reports a usage error.

At a terminal, search aligns the **key**, dim author/year and title. Titles
end with `…` at the terminal width (80 columns if unavailable). Keys wider
than a third of the screen get their own line, with the details indented;
keys wider than the screen stay intact. This leaves useful title space
at both 80 and 120 columns. Lists of three or more authors use the first
author followed by “et al.”. An empty search suggests a shorter query.
`--width 60` overrides detected width for human terminal rows. Citation keys
and citation text are never shortened; only titles and author lists shrink.
`--color auto` (the default) styles only a terminal stream, when `NO_COLOR`
(including an empty value) is unset and `TERM` is not `dumb`. stdout and
stderr are detected separately. `--color never` disables colour;
`--color always` explicitly requests coloured human rows even through a pipe.
For example: `ibis search lantern --file refs.bib --width 60 --color never`.

By default, redirected or piped stdout keeps the original uncoloured, untruncated TSV
and plain summaries; `--json` keeps the engine's full results and
`source_health`. Source notes on search stderr report only failed refreshes, naming the
library and the local time of the retained read, for example:
`ibis: note: Zotero could not be refreshed; showing results from 2:14 PM`.
Entry warnings stay in `libraries` and the diagnostic counts in `doctor`.
Terminal `libraries` groups actionable warnings under aligned library names,
shows the entry key (or title) and any known field, and lists the first 10
per library followed by `and N more; see --json`. Warning JSON adds an
optional `entry` display label and supplies known `field` names; existing
fields keep their meanings.

### Keys and more matches

`ibis search lantern --file refs.bib --keys` prints only citation keys, one
per line in rank order, including at a terminal. Keyless hits are skipped
and counted in one stderr note pointing to the same search with `--json`.
Keys that do not identify exactly one record across all open libraries are
also omitted, with an ambiguity count and the `--json`/`--id` route. A duplicate
beyond the requested search page still makes a bare key ambiguous.
An empty selection prints no bytes. `--keys` conflicts with `--json`;
both machine forms always omit decoration.

Search probes one extra candidate. When more matches exist, terminal stderr
says `note: showing the first 10 matches; see more with --limit 20` for the default limit of 10.
At the maximum of 1,000 it asks for more distinctive words instead.
`ibis search lantern --file refs.bib --limit 1 --json` includes
`"complete": false` if a second match exists, and true otherwise. This is
separate from `source_health`, has the same meaning in MCP, and changes no
exit code. Exact pasted key lists still return the whole selection.

Unknown keys give up to three close spelling suggestions from the same open
libraries, with bounded work. `ibis show vale2024lantern --file refs.bib`
might say `Did you mean 'vale2024lanterns'?` and
`Try: ibis show vale2024lanterns --file refs.bib`. Without a close key the
next command searches that library. JSON errors retain `code` and `reason`
and add `suggestions` (keys) and `next` (an argument array beginning with
`"ibis"`). `show`, `cite` and `entry` never substitute a suggested key.
The suggested `show KEY` must select one record in the whole open scope.
Otherwise `next` searches that key with `--json`; choose a result and use
`show --id ID_JSON` with the same library arguments.

### Citation keys from a list

```sh
ibis cite --keys-from keys.txt --file refs.bib --style apa
printf 'vale2024lanterns\n# another mention\nvale2024lanterns\n' | ibis cite --keys-from - --file refs.bib
```

`--keys-from FILE` reads UTF-8 keys, one per line. It trims surrounding
spaces and ignores blank lines and lines starting with `#`. Order and
duplicates are kept. `--keys-from -` explicitly reads stdin; when stdin is
a terminal, stderr says it is waiting for keys and to end with Ctrl-D.
Positional keys, `--request`, `--records` and per-key options conflict with
`--keys-from`. Input is at most 1 MiB and uses the existing citation item
limit; an unknown key or a refusal still withholds the whole citation.

<!-- Generated by node scripts/gen.mjs from the command line's own options. -->
### Every option

The options each command takes, as `ibis COMMAND --help` lists them.

**Every command** also takes:

| Option | Does |
| --- | --- |
| `--json` | Print the versioned JSON envelope, including errors and findings |
| `--color WHEN` | Colour human output: auto, always or never; auto respects each terminal, NO_COLOR and TERM |
| `--width N` | Use N columns for human terminal output instead of the detected width |
| `--contract VERSION` | The JSON contract version your client expects; one this Ibis does not speak is refused |
| `--zotero` | Read the local Zotero personal library, open or closed |
| `--app-libraries` | Read the libraries chosen in the Ibis app, read-only; search, show, cite, libraries and doctor do this when no library is named |

**`ibis libraries`**: List the libraries and their warnings

**`ibis search`**: Find references

| Option | Does |
| --- | --- |
| `--keys` | Print only citation keys, one per line in rank order |
| `WORDS` | Words from the author, title, year or citation key; quote several words |
| `--limit N` | Return at most N results, from 1 to 1000 |
| `--shorthand` | Read a signal before the words ("see") and a page after them ("417"), as the picker does |

**`ibis show`**: Show one reference

| Option | Does |
| --- | --- |
| `KEY` | The citation key of the reference |
| `--id ID_JSON` | The reference's id, as the JSON that ibis search --json prints |

**`ibis cite`**: Write a citation

| Option | Does |
| --- | --- |
| `KEY…` | Citation keys, in the order they appear in the citation |
| `--keys-from FILE` | Read one citation key per line from FILE, or - for stdin; ignore blanks and # comments |
| `--request` | Read the whole request from stdin as JSON; see: ibis schema cite |
| `--records` | Cite every CSL-JSON record in the array on stdin, with no library; see: ibis schema records |
| `--format FORMAT` | Output to write: a key syntax or a common style [default: pandoc]; list them with: ibis outputs |
| `--style STYLE_ID` | Format with any CSL style instead; list them with: ibis styles |
| `--project NAME` | Use this project's output and bibliography; list names with: ibis projects list |
| `--nightly` | Read the project or the app's formats from Ibis Nightly's settings |
| `--document PATH` | Use an explicit document's bibliography/csl front matter to select a project |
| `--strict` | Refuse citations with losses, refusals or source problems |
| `--for-app APP_ID` | Use the format Ibis uses for this app from its settings, a macOS bundle ID or Windows executable; --style or --format stands in for your usual format |
| `--bibliography` | Format bibliography entries, in --style or a style --format names |
| `--form FORM` | Citation form [default: normal] |
| `--keep-order` | Keep the keys in the order given where a style would sort them |
| `--locale LOCALE` | Language for --style output, such as de-DE; default the style's own. List them with: ibis styles --locales |
| `--locator LOCATOR` | Pinpoint the key before it, such as "p. 12", "ch. 3" or "12-14" |
| `--prefix TEXT` | Text before the key before it, such as "see" |
| `--suffix TEXT` | Text after the key before it |
| `--omit-author` | Leave out the author of the key before it |

**`ibis project`**: Check citation keys against a project's own .bib, read-only

| Option | Does |
| --- | --- |
| `BIB_FILE` | The .bib the document compiles against; it is never changed |
| `KEY…` | Citation keys, exactly as the document uses them |

**`ibis entry`**: Print a reference's original BibTeX entry, with what it needs to stand alone

| Option | Does |
| --- | --- |
| `KEY` | The citation key of the reference |

**`ibis outline`**: List a reference's attachments and whether Ibis can read their text

| Option | Does |
| --- | --- |
| `KEY` | The citation key of the reference |
| `--id ID_JSON` | The reference's id, as the JSON that ibis search --json prints |

**`ibis read`**: Print up to five pages of an attachment's text

| Option | Does |
| --- | --- |
| `ATTACHMENT` | The attachment's id, as ibis outline prints it |
| `--pages RANGE` | The pages to read, like 3 or 3-5; at most five are read at once |
| `--offset N` | Start N bytes into the first page, or into the text when pages are not known |

**`ibis find-in`**: Find short passages of an attachment that hold some words

| Option | Does |
| --- | --- |
| `ATTACHMENT` | The attachment's id, as ibis outline prints it |
| `WORDS` | The words to find, ignoring case; quote several words |

**`ibis share`**: Make a pack of references to send: a reading list, a .bib file and, if asked, their PDFs

| Option | Does |
| --- | --- |
| `KEY…` | Citation keys of the references, in order |
| `--keys-from FILE` | Read one citation key per line from FILE, or - for stdin; ignore blanks and # comments |
| `--to PATH` | Write the pack here, a new zip file or with --folder a new folder; nothing there is replaced |
| `--pdfs` | Add PDFs from the library folder or Zotero storage. Attached PDFs are copied as they are, including anything inside them. |
| `--folder` | Write a plain folder instead of a zip file |
| `--style STYLE_ID` | The CSL style of the reading list, default apa; list them with: ibis styles |
| `--locale LOCALE` | Language of the reading list, such as de-DE; default the style's own |

**`ibis styles`**: Find bundled CSL styles and citation languages

| Option | Does |
| --- | --- |
| `WORDS…` | Words from a style's title or id; every word must match |
| `--limit N` | Return at most N styles, up to 20000 |
| `--offset N` | Skip the first N styles |
| `--locales` | List citation languages instead of styles |

**`ibis outputs`**: List the outputs cite writes: key syntaxes, then the common styles

**`ibis schema`**: Print the JSON Schema of each method

| Option | Does |
| --- | --- |
| `METHOD` | Print only this method's schema; leave out for every method |

**`ibis call`**: Call a method with JSON parameters

| Option | Does |
| --- | --- |
| `METHOD` | The method, as ibis schema lists them |
| `--params JSON` | The method's parameters as one JSON object |
| `--stdin` | Read method parameters from stdin |

**`ibis serve`**: Serve JSON-RPC to other programs over stdio

| Option | Does |
| --- | --- |
| `--stdio` | Use stdin and stdout, the only transport |

**`ibis mcp`**: Serve read-only search and citation tools to AI agents (MCP over stdio)

| Option | Does |
| --- | --- |
| `--allow-writes` | Also offer tools that propose library changes; you apply or reject each one in the Ibis app |

**`ibis toggle`**: Show or hide the desktop picker

| Option | Does |
| --- | --- |
| `--app PATH` | The Ibis app to start when it is not running |

**`ibis doctor`**: Check your libraries and settings offline, without writing or sending anything

| Option | Does |
| --- | --- |
| `--config PATH` | Check a chosen Ibis settings file without repairing it or loading its libraries |

**`ibis agents`**: Connect Ibis to the AI apps on this computer, show them, or disconnect them

**`ibis agents setup`**: Add Ibis's read-only MCP server to each AI app found here

| Option | Does |
| --- | --- |
| `--client APP` | Only this app; repeat for several |
| `--dry-run` | Show what would change and change nothing |
| `--yes` | Do it without asking; needed when no one can answer, such as in a script |
| `--replace` | Replace a different ibis entry without asking |

**`ibis agents status`**: Show which AI apps have Ibis connected

| Option | Does |
| --- | --- |
| `--client APP` | Only this app; repeat for several |

**`ibis agents remove`**: Take Ibis's server out of each AI app; other servers are kept

| Option | Does |
| --- | --- |
| `--client APP` | Only this app; repeat for several |
| `--dry-run` | Show what would change and change nothing |
| `--yes` | Do it without asking; needed when no one can answer, such as in a script |

**`ibis settings`**: Back up the Ibis app's settings, or restore them on this or another computer

**`ibis settings export`**: Write a backup of the app's settings, without paste approvals or window places

| Option | Does |
| --- | --- |
| `FILE` | The backup file to write; without one, or with -, the backup goes to stdout |
| `--force` | Replace FILE if it exists |
| `--nightly` | Use Ibis Nightly's settings instead of the stable app's |

**`ibis settings import`**: Restore a backup into the app's settings, keeping the replaced settings as settings.json.bak

| Option | Does |
| --- | --- |
| `FILE` | The backup file to restore |
| `--dry-run` | Show what would change and change nothing |
| `--nightly` | Use Ibis Nightly's settings instead of the stable app's |

**`ibis projects`**: Remember a project's citation output and optional bibliography

**`ibis projects list`**: List project names, formats and bibliography file names

**`ibis projects schema`**: The JSON Schema of cite --project's result

**`ibis projects create`**: Create a project

| Option | Does |
| --- | --- |
| `NAME` | A unique name for this project |
| `--format FORMAT` | Output ID from ibis outputs: a key syntax or a common style |
| `--style STYLE_ID` | Any bundled CSL style ID; list them with: ibis styles |
| `--bibliography FILE` | The read-only project bibliography to check, as a .bib file |

**`ibis projects rename`**: Rename a project

| Option | Does |
| --- | --- |
| `NAME` | The project name printed by ibis projects list |
| `NAME` | A unique name for this project |

**`ibis projects remove`**: Remove a project and its app selections; keep its bibliography file

| Option | Does |
| --- | --- |
| `NAME` | The project name printed by ibis projects list |

**`ibis projects update`**: Change a project's output or bibliography

| Option | Does |
| --- | --- |
| `NAME` | The project name printed by ibis projects list |
| `--format FORMAT` | Output ID from ibis outputs: a key syntax or a common style |
| `--style STYLE_ID` | Any bundled CSL style ID; list them with: ibis styles |
| `--bibliography FILE` | The read-only project bibliography to check, as a .bib file |
| `--no-bibliography` | Stop checking a bibliography in this project |

**`ibis proposals`**: List, show or reject library changes AI apps proposed; apply them in the Ibis app

**`ibis proposals list`**: List proposals, newest first

**`ibis proposals show`**: Show what a proposal would change, with the entry before and after

| Option | Does |
| --- | --- |
| `ID` | The proposal id printed by ibis proposals list |

**`ibis proposals reject`**: Reject a pending proposal; no library changes

| Option | Does |
| --- | --- |
| `ID` | The proposal id printed by ibis proposals list |

**`ibis sources`**: Check that a library's source keeps Ibis's promises, or keep a folder's PDFs as references

| Option | Does |
| --- | --- |
| `ACTION` | check: read the libraries named here again and check what every source promises; keep: read each new PDF in the folders named with --file and keep what it says in the folder's ibis-references.bib |
| `--expect FILE` | CSL-JSON records the library should hold exactly, without their ids, in any order |
| `--changed-from REVISION` | A revision an earlier check printed: check that the library changed since |

**`ibis completions`**: Print shell completions

| Option | Does |
| --- | --- |
| `SHELL` | The shell to generate completions for |

**`ibis help`**: Print this message or the help of the given command

| Option | Does |
| --- | --- |
| `COMMAND` | The command to explain, such as cite |
<!-- End of generated options. -->

## Output for scripts and agents

What other programs may rely on, and what may change:

| Stable within a contract version | Free to change |
| --- | --- |
| `--json` envelopes and their `data`, as `ibis schema` describes them | Terminal output, its layout and wording |
| `call`, `serve` (JSON-RPC) and `mcp` requests and results | Message text; use `message.id` and its arguments |
| Error codes, reasons, recoveries and exit codes | Option help text and the order of options |
| Library ids, record ids and `revision` values, as opaque strings | How ids and revisions are computed |

Build on the left column. Terminal output is for people
([decision 0056](decisions/0056-open-layer-for-research-material.md)).

`--json` writes one envelope and newline to stdout, including help and
argument errors. Human explanations and warnings go to stderr. `serve` and
`mcp` stream their own protocols instead. Plain terminal output escapes
control characters; JSON preserves source strings.

```json
{"ibis":1,"ok":true,"data":{}}
{"ibis":1,"ok":false,"error":{"code":"not-found"}}
```

`ibis` is the contract version, now 1. `--contract 1` selects it; unsupported
versions are refused with the supported versions. Results may gain fields and
codes: ignore unknown fields and review unknown codes. Unknown request fields
are `bad-input`.

Until 1.0 the CLI, MCP tools and engine contract are unstable: any release may
change or remove any part of them, and each release lists its contract
differences ([decision 0028](decisions/0028-engine-first.md)).

`ibis schema METHOD` describes `data`; `cite --project` uses
`ibis projects schema`. Branch on `error.code` and optional
`reason`, not wording. Exit 1 has `ok: true` with findings; inspect outcomes
before using citation text. Empty search is success.

Errors also require `message: {id, args}`, one `recovery: {action, ...}` and
`focus`. Citation refusal outcomes and failed-refresh findings carry the same
presentation fields. Message ids resolve through `locales/en-US.ftl`; arguments
are data, never pre-rendered explanations. A recovery of `none` must give its
reason (`cancelled` or `in-progress`). Missing citation keys request
`choose-style` / `format-picker`; a moved or removed library requests
`locate-file` / `library-picker`. MCP tool errors expose these facts under
`structuredContent.error` in addition to their text explanation.

Library results (`libraries`, `search`, `show`, `cite`, `entry`, `project`)
carry `source_health` in JSON, RPC and MCP: `{"status":"ok","findings":[]}`
or status `findings` with one entry per problem. It describes the same
published state and only the libraries read: search's selected libraries,
every library for a key, or the item's own library.

`refresh-failed` means the result used the last good read; it gives `error`
and `last_successful_read_ms`. `source-warnings` gives the `count` of entries
skipped or read with problems. Later refreshes cannot change a result's health.

Exit 0 means success, including empty results. Exit 1 means findings: refusals,
losses, source warnings or `source_health` problems in libraries read.
An output-write failure is `internal`; a closed reader pipe is not an error.

<!-- gen:errors -->

| Exit | Error code |
| --- | --- |
| 2 | `usage` |
| 3 | `bad-input` |
| 4 | `not-found` |
| 5 | `source-unavailable` |
| 5 | `resource-limit` |
| 6 | `newer-format` |
| 70 | `internal` |
| 130 | `cancelled` |

<!-- /gen:errors -->

[Release notes](https://github.com/ibiscite/ibis/releases) call out contract changes;
`ibis schema` describes the version you have.

Input JSON is at most 1 MiB; duplicate members are refused. Responses are
bounded to 16 MiB. Ask for fewer results on `resource-limit`.

### Canonical citation requests

`cite --request` reads a `CitationRequest` from stdin; `call cite` takes it
as parameters. Items carry `ItemRef` and cite fingerprints from search/show,
with typed locators, affixes, form and output. Order and duplicates are kept.
See `ibis schema cite` for fields.

Outputs are key syntax, CSL style or preset (an `outputs` ID). Results include
text, supported HTML/RTF, display markup, outcomes, losses, notices and content
hash. Changed/deleted/ambiguous items are refused. JSON retains partial results;
plain output prints text only for a complete citation. See
[citation limits](compatibility.md#writing-apps) and
[supported editing](parity.md#shaping-one-citation).

### Records you send

`ibis cite --records` cites the CSL-JSON array on stdin without opening a
library, changing or remembering data, or needing a temporary file.

```sh
ibis cite --records --format apa < refs.json
ibis cite --records --style apa --bibliography < refs.json
```

It cites every record in order, with `cite`'s format, style, form, bibliography
and locale options; `--keep-order` is refused. The `records` method (`ibis call
records` or RPC) also takes `items`: record IDs with locators, prefixes and
suffixes. `ibis schema records` lists the fields. MCP's `cite` with `records`
cites every supplied record and refuses `items`, `for_app` and `keep_order`.

- Each record needs its own `id` and a CSL `type`; numeric IDs are preserved.
  Only `citation-key` is a key: without one, style formatting works but key
  output refuses the record with `missing-key`.
- A record that isn't valid CSL-JSON refuses the whole request (`bad-input`,
  `rejected-record`), and stderr names it, so no record is left out unseen. Two
  records with one `id`, `1` and `"1"` included, give `duplicate-record`.
- A `file` field is dropped unread: a path or `file:` URL is never looked at. Links
  with a login and local links are removed, as they are from library files. Fields
  outside CSL are dropped.
- At most 1,000 records, 512 KiB of them, nested at most 16 deep; beyond that is
  `resource-limit`, as is input over 1 MiB. JSON nested more than 128 levels can't be
  read at all and is `bad-input`.
- A field that can't be read as sent, a field outside CSL or a removed link is
  dropped with a warning, and the citation exits 1, as it does for a library file.

Input is one UTF-8 JSON value, read to EOF with a 1 MiB limit. stdout holds
the citation or `--json` envelope; reasons go to stderr. RPC cancellation
answers `cancelled` without affecting the next request.

### A project's own bibliography

```sh
ibis project thesis.bib vale2024lanterns fen2023clouds --file library.bib
ibis entry vale2024lanterns --file library.bib
```

`project` checks exact keys without adding the bibliography to searched
libraries or changing it. It reports in-file, missing, repeated, doubtful or
uncheckable keys. Exit 0 means every key is present; 1 means findings. JSON
includes status and lines. In-file is not proof of matching fields or a
compiling document. A changing/damaged file cannot establish missing keys;
case variants, aliases and data-only entries are not exact matches. A named
source library also enables metadata comparison.

`entry` prints the original BibTeX entry and required strings/dependencies.
It refuses unavailable originals, changed files, repeated keys and missing or
redefined dependencies. A refusal is a finding, as a refused citation is: exit
1, with `ok: true`, `data.outcome: "refused"` and a `reason` in `--json`.
`no-original` means the library keeps no BibTeX source (Zotero, CSL-JSON, RIS
and CSV); `ibis cite KEY --format bibtex` writes an entry from the record's
fields instead. Original entries may include private `file`/`note` fields;
inspect them before sharing.

To use the same `.bib` elsewhere, upload it beside an Overleaf document, name it
under Quarto's `bibliography:` or use Typst's `#bibliography("thesis.bib")`.
The check covers only the file you name.

### Attachment text

```sh
ibis outline vale2024lanterns --zotero
ibis read ATTACHMENT --pages 3-5 --zotero
ibis find-in ATTACHMENT "brass lantern" --zotero
```

`outline` lists a reference's attachments: an id for each, its title and
type, and whether Ibis can read its text. `read` prints at most five pages or
32 KiB, each page marked; `next` in `--json` (or the last line) says how to
go on. `find-in` prints at most ten passages of about 400 characters that
hold the words, ignoring case and line breaks, with their pages.

CLI and MCP attachment text tools refuse files whose resolved path is outside the `.bib` library’s folder or Zotero’s storage folder unless **Allow attachment text outside the library’s folder** is enabled in **Settings › AI Apps › Attachment text** (`attachment_text_outside_library`, device-only and off by default).

Text comes from two places. For a Zotero attachment Zotero indexed, it is the
text Zotero keeps: while Zotero runs, Ibis asks Zotero for it; while Zotero is
closed, it reads Zotero's text cache from the attachment's storage folder,
with the page counts from its private copy of the database. Zotero's pages
count only when the text's page breaks match the number of pages Zotero
recorded; both indexed and total counts must be present and equal. A PDF
with no such index, or with only part of the file indexed, and a `.bib`
file's `file` attachments that are PDFs, are read by Ibis itself:
`ibis` starts a short-lived copy of itself that gets the opened file and
nothing else, no environment and no path, parses it in at most ten seconds
and gives each page's text. It may use at most 2 GiB of memory, and on Linux
3 GiB of address space; a PDF that needs more is `too-large`. The desktop app never
parses a PDF. Nothing is written, and the text stays in memory for the one
command.

Ibis reads only a regular file on this computer of at most 256 MiB and 10,000
pages that starts as a PDF: never a link, a file on a network or FUSE mount,
or an online-only file not yet downloaded. A Zotero file is looked for in its
storage folder, or at a linked file's full path; while Zotero runs, only in
the data directory Ibis finds on this computer. A linked file relative to
Zotero's base folder is not read. `outline` says why a file's text is
not read: `missing`, `not-local`, `not-pdf`, `damaged`, `too-large` or
`timed-out`. Text without known pages is read by `--offset` and has no page
numbers.

`pdf_page` is the file's own page, from 1; use it only to find a passage.
`cite_page`, the printed page to cite, comes only from the PDF's page labels
(`/PageLabels`), such as `xii` or `A-3`: at most 16 letters, digits, `-` or
`.`, and never a label two pages share. With Zotero's text, labels count only
when the PDF has exactly as many pages as Zotero recorded. Otherwise
`cite_page` is null and the printed page is not known; it is never taken from
the file's page number.

An attachment id names one attachment while its library's content stays the
same: reloading an unchanged library keeps it. Changing the library, the
attachment, its indexed text or its PDF file retires the id; run `outline`
again after a replacement or reindex. A file that cannot be read at the moment
is reported as such, not as a retired id. It holds no path
or file location. Links have no text, and EPUB and web snapshots have text only from Zotero's index. Zotero shows
Reindex Item for a file with no text.

AI apps need a separate choice for each library, off by default. In
**Settings › AI Apps › Attachment text**, turn on **Let AI apps read attachment
text** for the library. The app and CLI share this device-only setting:
`"attachment_text": ["lib_example_library"]` names complete opaque library
ids from `ibis libraries --json`, including the Zotero profile's identity.
Choosing one profile's personal library or group never opens another profile's.
This choice stays on this computer and out of settings backups. The direct
commands and `ibis call outline|read|find-in` use the app's libraries when none
is named and do not need this choice.

### A pack of references

```sh
ibis share vale2024lanterns fen2023clouds --pdfs
ibis share vale2024lanterns fen2023clouds --pdfs --to "Reading pack.zip"
ibis share --keys-from keys.txt --style ieee --folder --to pack
```

`share` makes one pack to send someone ([#1953](https://github.com/ibiscite/ibis/issues/1953)):
`Reading list.html`, a page any browser opens with each reference written in
the style (APA unless `--style` names another) and its DOI or address as a
link; `References.bib`, which loads the references into any reference
manager; and, only with `--pdfs`, a `PDFs` folder with each reference's PDFs
named like `Aster 2026 - Title.pdf`. Names suit macOS, Windows and Linux; two
names one of them would take for the same get ` (2)`, ` (3)` in order. A
reference picked twice is packed once.

Without `--to`, nothing is written: it lists the files and their sizes, as
the app shows them before saving. `--to` names a new zip file, or with
`--folder` a new folder; a file or folder already there is refused
(`destination-exists`) and never replaced. The zip stores its files as they
are, since PDFs are already compressed, and a pack holds at most 2 GiB.

The reading list, BibTeX metadata and filenames omit library notes, annotations,
private links and file locations, including paths into named home directories.
BibTeX preserves valid citation keys exactly as stored. Attached PDFs are copied
as they are, including anything inside them. PDFs come only from inside the
`.bib` library's folder or Zotero's storage, opened as `read`
opens them (a regular local file, never a link) and at most 256 MiB each,
whatever **Allow attachment text outside the library’s folder** says. A
reference the style cannot write, one without a usable citation key for the
`.bib`, or one whose PDFs cannot be added is named on stderr, with exit
status 1; `--json` gives their places in the selection as `not_in_list`,
`not_in_bibtex` and `without_pdf`. No answer holds the pack's location.
`ibis call share` and JSON-RPC take the same request, with an absolute
`destination`; MCP has no share tool.

### Projects

A project saves a name, format and optional read-only `.bib` in `settings.json` (under `writings`)
([decision 0042](decisions/0042-formats-apps-and-writings.md)). CLI citations use
only an explicitly named project or supplied document context; `--for-app ID`
explicitly uses that app's project and bibliography.

```sh
ibis projects create "Field Guide" --format autocite --bibliography refs.bib
ibis projects list --json
ibis cite vale2024lanterns --file library.bib --project "Field Guide"
ibis cite vale2024lanterns --file library.bib --document chapter.qmd
ibis projects rename "Field Guide" "Revised Field Guide"
ibis projects update "Revised Field Guide" --style apa --no-bibliography
ibis projects remove "Revised Field Guide"
```

Create requires `--format` or `--style`; update retains omitted fields.
Rename updates app selections; remove leaves files intact. Definitions and
selections stay on this computer, outside backups. `--nightly` selects Nightly's
settings. Busy or failed saves preserve the old file. Handwritten definitions
need no ID or revision:

```json
{"name":"Thesis","format":"apa","bibliography":"/invented/work/refs.bib"}
```

`--project` conflicts with `--format`, `--style`, `--for-app` and `--request`.
`--document PATH` reads only the supplied Markdown/Quarto document's `bibliography`
and `csl` front matter, using plain or quoted scalars. It selects a unique matching
project; ambiguity refuses. Nearby files, names, titles and URLs never select one.

Only key outputs check the bibliography. Missing, uncertain or unreadable entries
keep citation text with exit 1; a known conflicting DOI withholds it. `--strict`
returns an error unless a complete current read finds every emitted key
unambiguously. It changes no setting and grants no paste permission.

Project JSON wraps `citation` with `name`, `assessment` (`ready` or `conflict`),
`bibliography` (the check or null) and `source_health`. Conflicts have typed refusals
and no copyable text; strict failures return an error envelope.
`ibis projects schema --json` describes it. Only bibliography file names are shown.
Plain `cite` has no wrapper.

### Receipts and strict citations

Complete citations in JSON and RPC carry `receipt`, and MCP's `cite` does with
`include_receipt: true`: ordered opaque record
identities, cite fingerprints, resolved options, source read times/findings,
content hash and formatting versions. Caller snapshots have no source read time;
caller-record receipts list no library reads and have clean `source_health`.
CSL adds the actual language and processor/style/locale bundle digests. Writings
add an opaque definition hash, bibliography fingerprint and successful check time.
Record and annotation identities contain no native keys or paths and cannot
select records later. Receipts describe evidence; they do not archive records
or promise historical recreation.

`ibis cite --strict` returns an error without text for any loss, refusal,
source warning or failed refresh. `--request` accepts the flag or `"strict": true`,
as do RPC requests; MCP's `cite` accepts `strict`. Caller records apply it to
supplied data, including unreadable-field warnings. Writing key outputs also
require a complete current bibliography check finding every emitted key
unambiguously. Strict mode changes no saved setting. Failures have reason
`strict-citation` and exit 3 (`bad-input`), or 5 (`source-unavailable`) for a
failed refresh.

### Output for an app

`ibis cite KEY --for-app ID` reads the app's settings: its selected writing,
then its remembered format, your format for its kind, then a suggestion.
Documents use styles; Markdown, LaTeX, Typst and code editors use key syntaxes.
IDs are macOS bundle IDs or Windows executable names, with or without folders,
on any OS (for example `com.microsoft.Word` or `WINWORD.EXE` suggests APA,
`md.obsidian` Pandoc, `TeXShop` BibLaTeX). `--format`/`--style` stands in for
your usual format of that kind; `--nightly` selects Nightly settings.
Unreadable settings stop the command. MCP's `for_app` follows the same rule.

JSON adds `destination`: app, known `name`, `output`, `from` (`writing`, `app`,
`yours` or `suggestion`), selected `writing`, and `delivery` (`paste` or `copy`
with its reason, with pasting on and permitted). Writing citations put this
under `citation.destination`. This describes policy, not a paste action.
Plain output remains the citation with a stderr note.

## JSON-RPC server

`ibis serve --stdio --file refs.bib` uses JSON-RPC 2.0 with LSP
`Content-Length` framing. It sends library-change notifications and handles
`$/cancelRequest`, including queued work. Requests execute in order behind
an eight-request queue. Arrays receive Invalid Request, not batch execution.
Errors put Ibis codes in `error.data`; stdout contains only protocol frames.
If libraries fail at startup, it writes a framed error and exits.

Every method in `ibis schema` is served. Replies are written whole or not at
all. Frames over 1 MiB get a parse error and stop the server; invalid JSON
in a complete frame gets a parse error and reading continues. At EOF, read
requests finish before exit. Process exit codes do not apply per request.

## AI agents (MCP)

**Settings › AI Apps** and `ibis agents` share the engine and saved app
settings. Connect, disconnect, repair a stale path, or review and replace a
different entry with the row’s button. **Command line** installs `ibis`.

```sh
ibis agents setup            # asks first
ibis agents status           # which apps have Ibis
ibis agents remove           # undo; other servers stay
```

Setup finds Claude Code, Claude Desktop, Codex, Cursor, VS Code, Gemini CLI,
Windsurf and LM Studio, using `ibis mcp --app-libraries`. Claude Code and Codex
use their own `mcp add` commands. Other apps get a JSON merge that preserves
other servers and settings, with a private `<file>.ibis-backup-N` for each
change; older backups stay. Links, non-JSON and files changed during setup are
left alone. Claude Desktop and LM Studio need a durable full CLI path. The
Ibis plugin already connects Claude Code, so setup adds nothing there.
ChatGPT desktop shows steps for its own window.

Status reports `connected`, `not-connected`, `stale`, `different`, `blocked`
or `by-hand`; `stale`, `different` and `blocked` exit 1. Setup repairs its own
stale path without a replacement question, reporting `replaced` and the diff.
Different entries require agreement or `--replace`; removal takes out only
setup’s own entry, including a stale path. Scripts use `--yes`; `--dry-run`
changes nothing.

For any other app, follow the [assistant setup guide](https://ibiscite.org/setup),
or paste this prompt into your assistant:

```text
Set up Ibis for me: https://ibiscite.org/setup
```

`ibis mcp --file refs.bib`, or `ibis mcp --app-libraries`, serves these
read-only tools over stdio:

| Tool | Input and result |
| --- | --- |
| `search` | `query`, optional `limit` (1–25, default 10), `shorthand` for a signal and locator; short hits with checked `ref` values, `complete` and `source_health`; shorthand adds `cited_with` and `searched_for`. |
| `show` | `ref`; bibliographic fields and `source_health`. |
| `cite` | Items with `ref` and optional locator/affixes/author omission, or caller [records](#records-you-send); a key-syntax `format` or any CSL `style`, locale, form, order, optional `for_app`, `strict` and `include_receipt`; citation text, notices, losses, warnings, `source_health`, and the receipt when asked. |
| `styles` | Optional `query`, `limit` and `offset`; matching styles, total and locales. |
| `outline` | `ref`; the reference's attachments, each with an `id` for `read` and `find_in` and whether its text can be read. |
| `read` | `attachment`, optional `first_page`, `last_page` and `offset`; at most 5 pages or 32 KiB, each with `pdf_page` and `cite_page`, and `next`. |
| `find_in` | `attachment` and `query`; at most 10 passages of about 400 characters, each with `pdf_page` and `cite_page`, and `complete`. |

`outline`, `read` and `find_in` are listed only when the person lets AI apps
read attachment text from an open library, and they answer only for such a
library. The choice is per library, off by default, and kept in the app's
settings as `"attachment_text": ["lib_example_library"]`, using complete
library ids from `ibis libraries --json`, including the Zotero profile;
it stays on this computer and is not part of a settings backup. While they
are listed, `tools/list` answers with `cacheScope: private`. The `ibis outline`,
`ibis read` and `ibis find-in` commands do not need this choice.

Each answer's `structuredContent` holds the full result. Its text is a short
summary for the model instead of the same JSON again: one `ref · authors ·
year · title` line per hit, each notice, loss and warning in words, and any
library notes. For `cite`, the first line is `Citation:` and the citation as
one JSON string, so a title cannot start a line and imitate a warning.
Receipt, bibliography and delivery advice are named in that summary too.
The common styles are accepted as
`format` too, but the schema lists only key syntaxes; `styles` finds the rest.

Refs contain identity and a short content check; changed records are refused.
When a key is missing or shared, the ref uses an opaque digest. It contains no
native record key and remains usable with `show` and `cite` in the open snapshot.
Unknown arguments fail. Failed library startup leaves MCP running with an
explanation on each library-dependent tool. Library text carries provenance
marking it as data, and source-health notes report incomplete/stale results.

Supported protocol revisions are `2026-07-28`, `2025-11-25` and `2025-06-18`.
The newest uses per-request metadata and `server/discover`; older clients use
`initialize`. Unsupported explicit revisions are refused; initialize negotiates
an available legacy revision.

MCP excludes notes, annotations, source paths, local links and credentialed
addresses. Bibliographic field overrides in Extra still count as fields.
Search/show remove markup and unsafe invisible characters; cite returns the
printed citation. Values and answers are bounded (256 KiB per answer). Raw
CLI search/show JSON is not this privacy projection. MCP opens no external
connection; an AI client can send received data to its own service.

The repository's [plugin](../plugins/ibis/skills/ibis-cite/SKILL.md) supplies
MCP configuration and a citation skill for Claude Code/Codex. Its server entry
is the one `ibis agents setup` writes. It requires `ibis` on PATH and does not
install the executable. It reads app libraries,
stops if MCP is unavailable and requires confirmation of partial/typo matches.
The skill directory can also be copied to a client's skill folder. Marketplace
or directory approval is not established by the repository configuration.

Settings backup/restore instructions live in
[moving to a new computer](moving-to-a-new-computer.md).

### Proposed changes

`ibis mcp --allow-writes`, with the usual library options, adds tools that
let an AI app propose small library changes
([decision 0055](decisions/0055-agent-layers-full-text-writes-remote.md)). They
only propose: nothing in a library changes until the person applies the
proposal in **Settings › AI Apps › Proposed changes**, and an AI client's own approval of a tool call never
counts. Without the flag these tools are not listed and a call is an unknown
tool. `ibis agents setup` writes the read-only server; add `--allow-writes` to
the `ibis mcp` arguments in an app's settings by hand to offer them.

| Tool | Input and result |
| --- | --- |
| `propose_add` | `fields` as CSL-JSON (`type` and `title` required) and `library` when several are open; the proposal id, a summary and the BibTeX entry Ibis would add. |
| `propose_change` | `ref` and CSL-JSON `fields` with new values; the proposal id and a summary. |
| `propose_tags` | `ref`, `add` and `remove` (BibTeX `keywords`, at most 20 tags of 64 characters); the proposal id and a summary. |
| `proposals` | Nothing; this app's proposals with their status, and the open libraries `propose_add` can name. |

Fields go through the same filter as every shared record: a field it would
drop, such as `note`, `file`, `id` or a private link, refuses the proposal by
name. A new reference whose DOI the library already holds is refused. Each
proposal is a private file beside the app's settings, at most 64 KiB and at
most 100 waiting ([privacy](../PRIVACY.md)).

```sh
ibis proposals list
ibis proposals show 3f9c2a7d41b8e605
ibis proposals reject 3f9c2a7d41b8e605
```

`show` prints the change and, for a `.bib` file, the entry as it is and as it
would be written. There is no `apply` command: the person applies a proposal
in **Settings › AI Apps › Proposed changes**, after the same preview. Applying writes `.bib` files only. It
refuses when the entry changed since the proposal, edits only that entry's
fields, or adds the new entry at the end, keeps every other byte, and saves
the replaced file as `refs.bib.ibis-backup-N`. A field written with a
`@string` macro is replaced whole; tags already written keep their spelling.
A new entry gets a citation key such as `taylor2026signals`: the first
author's family name, the year and the first title word that is not a short
word like "the", in lowercase ASCII, with `a`, `b`, … added when the file or a
waiting proposal already uses it in any letter case.

Only `.bib` libraries take proposals for now. Zotero libraries wait for
Zotero's official local write API, and Ibis never writes `zotero.sqlite`.
Applies run one at a time, and a library file reached through a link that
was not there when it was opened is left alone.
