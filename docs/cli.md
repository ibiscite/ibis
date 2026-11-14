# Command line reference

`ibis` searches libraries and prepares citations without the desktop app.
`ibis --help` and `ibis COMMAND --help` describe the installed version.

## Install

The desktop app includes the matching `ibis` built from the same commit.

- **macOS and Linux deb/rpm:** Settings › General › **Install the ibis command**
  links `~/.local/bin/ibis` to the command inside the app's bundle or package.
  Keep the macOS app in its permanent location. Install creates the link even
  when the app's PATH omits it. If that directory is missing from the app's
  PATH, copy this line into your shell profile, then reopen your terminal:
  `export PATH="$HOME/.local/bin:$PATH"`. Ibis never requests administrator
  access or changes your profile. Linux packages install no command on PATH
  as root. On macOS, Finder's PATH can differ from your terminal; Homebrew's
  standard locations are also checked for conflicts.
- **Windows:** the per-user NSIS installer adds the app folder to your user
  PATH without elevation. MSI also adds a user PATH entry. Open a new terminal
  after installation. Channel installs serialize their PATH updates and refuse
  additions that exceed Windows's expanded PATH limit. Successful uninstall
  removes the installer's entry; cancellation preserves it, and MSI rollback
  preserves later PATH edits.
- **Linux AppImage:** the same Settings action offers the command as an owned
  copy in `~/.local/bin`, so it keeps working after the image unmounts. It shows
  the same profile line when that directory is missing from PATH. Install again
  after an app update to update this copy (remove the old command first).

Settings shows the command's location, version and whether it matches this app.
Another command (including Homebrew) is never overwritten or run to inspect
its version; its location is shown with version unavailable. Unix PATH lookup
skips files without executable access. **Remove Command**
removes only an unchanged Ibis-owned installation and its receipt. Removing the
app itself on macOS cannot run cleanup: remove the command in Settings first.
The shared installation operation is available for `ibis agents setup` in
issue #1867's next slice; that command is not part of this slice.

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
| `--app-libraries` | Libraries chosen in the desktop app. |

Library options before or after the command combine in order; duplicates are ignored. One-shot reads install no watches
and never change library files. Library IDs are opaque and repeatable on one
computer; [architecture](architecture.md#data-and-identity) explains identity.
Library commands save only the private identity key; explicit `settings` and
`writings` commands also write the settings you choose.

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
| `--app-libraries` | Read the libraries chosen in the Ibis app, read-only; search, show, cite and libraries do this when no library is named |

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
| `--writing NAME` | Use this writing's output and bibliography; list names with: ibis writings list |
| `--nightly` | Read the writing or the app's formats from Ibis Nightly's settings |
| `--document PATH` | Use an explicit document's bibliography/csl front matter to select a writing |
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

**`ibis toggle`**: Show or hide the desktop picker

| Option | Does |
| --- | --- |
| `--app PATH` | The Ibis app to start when it is not running |

**`ibis doctor`**: Check your libraries and settings offline, without writing or sending anything

| Option | Does |
| --- | --- |
| `--config PATH` | Check a chosen Ibis settings file without repairing it or loading its libraries |

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

**`ibis writings`**: Remember a writing's citation output and optional bibliography

**`ibis writings list`**: List writing names, formats and bibliography file names

**`ibis writings schema`**: The JSON Schema of cite --writing's result

**`ibis writings create`**: Create a writing

| Option | Does |
| --- | --- |
| `NAME` | A unique name for this writing |
| `--format FORMAT` | Output ID from ibis outputs: a key syntax or a common style |
| `--style STYLE_ID` | Any bundled CSL style ID; list them with: ibis styles |
| `--bibliography FILE` | The read-only writing bibliography to check, as a .bib file |

**`ibis writings rename`**: Rename a writing

| Option | Does |
| --- | --- |
| `NAME` | The writing name printed by ibis writings list |
| `NAME` | A unique name for this writing |

**`ibis writings remove`**: Remove a writing and its app selections; keep its bibliography file

| Option | Does |
| --- | --- |
| `NAME` | The writing name printed by ibis writings list |

**`ibis writings update`**: Change a writing's output or bibliography

| Option | Does |
| --- | --- |
| `NAME` | The writing name printed by ibis writings list |
| `--format FORMAT` | Output ID from ibis outputs: a key syntax or a common style |
| `--style STYLE_ID` | Any bundled CSL style ID; list them with: ibis styles |
| `--bibliography FILE` | The read-only writing bibliography to check, as a .bib file |
| `--no-bibliography` | Stop checking a bibliography in this writing |

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

`--json` writes one envelope and newline to stdout, including help and
argument errors. Human explanations and warnings go to stderr. `serve` and
`mcp` stream their own protocols instead. Plain terminal output escapes
control characters; JSON preserves source strings.

```json
{"ibis":1,"ok":true,"data":{}}
{"ibis":1,"ok":false,"error":{"code":"not-found"}}
```

`ibis` is the contract version, now 1. `--contract 1` selects it; unsupported
versions are refused with the supported versions. Within a version, results
may gain fields and codes: ignore unknown fields and review unknown codes.
Other changes require a new version ([decision 0028](decisions/0028-engine-first.md)).
Unknown request fields are `bad-input`.

Older contracts stay supported for at least two minor releases.

`ibis schema METHOD` describes `data`; `cite --writing` uses
`ibis writings schema`. Branch on `error.code` and optional
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

| Exit | Meaning / error code |
| --- | --- |
| 0 | Success, including empty results. |
| 1 | Findings: refusals, losses, source warnings or `source_health` problems in libraries read. |
| 2 | `usage` |
| 3 | `bad-input` |
| 4 | `not-found` |
| 5 | `source-unavailable` or `resource-limit` |
| 6 | `newer-format` |
| 70 | `internal`, including output-write failure. A closed reader pipe is not an error. |
| 130 | `cancelled` |

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
redefined dependencies. Original entries may include private `file`/`note`
fields; inspect them before sharing.

To use the same `.bib` elsewhere, upload it beside an Overleaf document, name it
under Quarto's `bibliography:` or use Typst's `#bibliography("thesis.bib")`.
The check covers only the file you name.

### Writings

A writing saves a name, format and optional read-only `.bib` in `settings.json`
([decision 0042](decisions/0042-formats-apps-and-writings.md)). CLI citations use
only an explicitly named writing or supplied document context; `--for-app ID`
explicitly uses that app's writing and bibliography.

```sh
ibis writings create "Field Guide" --format autocite --bibliography refs.bib
ibis writings list --json
ibis cite vale2024lanterns --file library.bib --writing "Field Guide"
ibis cite vale2024lanterns --file library.bib --document chapter.qmd
ibis writings rename "Field Guide" "Revised Field Guide"
ibis writings update "Revised Field Guide" --style apa --no-bibliography
ibis writings remove "Revised Field Guide"
```

Create requires `--format` or `--style`; update retains omitted fields.
Rename updates app selections; remove leaves files intact. Definitions and
selections stay on this computer, outside backups. `--nightly` selects Nightly's
settings. Busy or failed saves preserve the old file. Handwritten definitions
need no ID or revision:

```json
{"name":"Thesis","format":"apa","bibliography":"/invented/work/refs.bib"}
```

`--writing` conflicts with `--format`, `--style`, `--for-app` and `--request`.
`--document PATH` reads only the supplied Markdown/Quarto document's `bibliography`
and `csl` front matter, using plain or quoted scalars. It selects a unique matching
writing; ambiguity refuses. Nearby files, names, titles and URLs never select one.

Only key outputs check the bibliography. Missing, uncertain or unreadable entries
keep citation text with exit 1; a known conflicting DOI withholds it. `--strict`
returns an error unless a complete current read finds every emitted key
unambiguously. It changes no setting and grants no paste permission.

Writing JSON wraps `citation` with `name`, `assessment` (`ready` or `conflict`),
`bibliography` (the check or null) and `source_health`. Conflicts have typed refusals
and no copyable text; strict failures return an error envelope.
`ibis writings schema --json` describes it. Only bibliography file names are shown.
Plain `cite` has no wrapper.

### Receipts and strict citations

Complete citations in JSON, RPC and MCP carry `receipt`: ordered opaque record
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

Follow the [assistant setup guide](https://ibiscite.org/setup) to connect your app
to Ibis, or paste this prompt into your assistant:

```text
Set up Ibis for me: https://ibiscite.org/setup
```

`ibis mcp --file refs.bib`, or `ibis mcp --app-libraries`, serves four
read-only tools over stdio:

| Tool | Input and result |
| --- | --- |
| `search` | `query`, optional `limit` (1–25, default 10), `shorthand` for a signal and locator; short hits with checked `ref` values, `complete` and `source_health`; shorthand adds `cited_with` and `searched_for`. |
| `show` | `ref`; bibliographic fields and `source_health`. |
| `cite` | Items with `ref` and optional locator/affixes/author omission, or caller [records](#records-you-send); format or style, locale, form, order, optional `for_app` and `strict`; citation text, notices, losses, warnings, receipt and `source_health`. |
| `styles` | Optional `query`, `limit` and `offset`; matching styles, total and locales. |

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
MCP configuration and a citation skill for Claude Code/Codex. It requires
`ibis` on PATH and does not install the executable. It reads app libraries,
stops if MCP is unavailable and requires confirmation of partial/typo matches.
The skill directory can also be copied to a client's skill folder. Marketplace
or directory approval is not established by the repository configuration.

Settings backup/restore instructions live in
[moving to a new computer](moving-to-a-new-computer.md).
