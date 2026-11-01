# Command line reference

`ibis` is the Ibis engine as a command. It does everything the app does with your
libraries, for people in a terminal, for scripts and for AI agents. The app is one
client of the same engine; nothing here needs the app to be installed or running.

## Install

Download `ibis` from the [nightly](https://github.com/ibiscite/ibis/releases/tag/nightly)
or a [release](https://github.com/ibiscite/ibis/releases/latest):
`ibis-macOS-Apple-silicon.tar.gz`, `ibis-macOS-Intel.tar.gz`, `ibis-Windows.zip` or
`ibis-Linux.tar.gz`. Unpack it and put `ibis` on your PATH. The macOS binary is not
signed yet; if macOS refuses to run it, run `xattr -d com.apple.quarantine ibis` once.
From a checkout, `cargo run -p ibis -- <command>` runs the same program.

`ibis --help` and `ibis <command> --help` describe every command and option.

## Libraries

Each run reads the libraries you name and nothing else:

| Option | Reads |
| --- | --- |
| `--file PATH` | A BibTeX, BibLaTeX or CSL-JSON file. Repeat for several. |
| `--zotero` | The local Zotero personal library, whether Zotero is open or closed. |
| `--zotero-group GROUP_ID` | A local Zotero group library. Repeat for several. |

Library options may come before or after the command; they add up in that order and
duplicates are ignored. Reading never writes to a library, and one-shot commands
watch no files. Library IDs in the output are opaque digests, never paths.

## Commands

| Command | Does |
| --- | --- |
| `search WORDS [--limit N]` | Find references by author, title, year or key. An empty result is a success. |
| `show KEY` or `show --id ITEM_REF_JSON` | One reference's full data. |
| `cite KEY…` | A citation. Key syntaxes with `--format` (default `pandoc`), formatted text with `--style` (see `styles`), bibliography entries with `--style S --bibliography`. `--locator`, `--prefix`, `--suffix` and `--omit-author` apply to the key before them. |
| `cite --request` | A citation from one canonical `CitationRequest` JSON object on stdin. |
| `libraries` | The loaded libraries, their item counts and typed source warnings. |
| `styles` | The bundled CSL styles. |
| `schema [METHOD]` | The JSON Schema of each engine method's parameters and result. |
| `call METHOD --params JSON` or `--stdin` | Any engine method with raw JSON parameters. |
| `serve --stdio` | A JSON-RPC 2.0 server for programs ([below](#json-rpc-server)). |
| `mcp` | A Model Context Protocol server for AI agents ([below](#ai-agents-mcp)). |
| `doctor` | An offline health report on the named sources and settings file ([terminal kit](../crates/ibis-cli/README.md#offline-diagnosis)). |
| `completions bash\|zsh\|fish` | Shell completions ([terminal kit](../crates/ibis-cli/README.md#shell-completions)). |
| `toggle [--app PATH]` | Show or hide the desktop picker. |

## Output for scripts and agents

Without `--json`, commands print short lines for people. Add `--json` to any
command, including `--help`, `--version` and argument errors, and stdout holds exactly
one JSON envelope followed by a newline. The exceptions are `serve` and `mcp`, which
stream their own protocols on stdout (below) and ignore `--json`:

```json
{"ibis":1,"ok":true,"data":{…}}
{"ibis":1,"ok":false,"error":{"code":"not-found"}}
```

- `ibis` is the envelope version. It changes only when the envelope itself changes.
- `data` is the method's result, described by `ibis schema METHOD`.
- `error.code` is stable and is the thing to branch on. The sentence that explains it,
  naming the argument, key or file at fault, goes to stderr as one `ibis: …` line, in
  both modes. Sentences may be reworded; codes are not.
- stderr also carries notes, for example when a library has warnings. stdout never
  carries anything but the envelope or the plain result.
- In terminal output, control characters and invisible direction marks from library
  data are escaped; `--json` strings are exactly what the library holds.

| Exit | `error.code` | Meaning |
| --- | --- | --- |
| 0 | | Success, including an empty search. |
| 1 | | Done with findings: refusals, losses, source warnings or a failed refresh. `ok` is `true` and `data` says what. |
| 2 | `usage` | The command line is not valid. |
| 3 | `bad-input` | The input JSON or a value is not valid. |
| 4 | `not-found` | A named key, item or library is not there. |
| 5 | `source-unavailable` | A library could not be read right now. |
| 5 | `resource-limit` | The request or its result is too large. |
| 6 | `newer-format` | A newer Ibis wrote this data. |
| 70 | `internal` | A bug in Ibis; please [report it](https://github.com/ibiscite/ibis/issues). |
| 130 | `cancelled` | The request was cancelled. |

Input JSON is at most 1 MiB and duplicate members are refused. A response is at most
16 MiB; a larger one is a `resource-limit` error, so ask for fewer results.

Until 1.0, fields inside `data` may still change between releases; the release notes
say when they do, and `ibis schema` always describes the version you have.

### Examples

```sh
# Titles of the first five matches
ibis search "memory" --file refs.bib --limit 5 --json | jq -r '.data.hits[].item.object.data.csl.title'

# Cite the first hit in APA
key=$(ibis search "garcia 2021" --file refs.bib --json | jq -r '.data.hits[0].item.object.data.citation_keys[0].value')
ibis cite "$key" --file refs.bib --style apa

# Branch on the outcome, not on the wording
if ! out=$(ibis cite smith2020 --file refs.bib --json); then
  [ "$(jq -r .error.code <<<"$out")" = not-found ] && echo "no such key"
fi
```

### Canonical citation requests

`cite --request` and `call cite` take the same `CitationRequest` the app uses: items by
`ItemRef` (from `search --json` or `show --json`) with their cite fingerprints, typed
locators, styled prefixes and suffixes, a form and an output. `ibis schema cite` lists
every field. Order and repeated items are kept. The result carries the citation text,
HTML and RTF where the output has them, a display tree, per-item outcomes, losses,
notices and a content hash. A changed, deleted or ambiguous item is refused instead of
guessed. Incomplete results stay inspectable in `--json`; plain output prints citation
text only when the citation is complete, and reports notices and omitted fields on
stderr.

Styles that need a whole document return a full reference entry with that
explanation. An output Ibis cannot write correctly is an explicit per-item
`unsupported-output` refusal.

## JSON-RPC server

`ibis serve --stdio --file refs.bib` speaks JSON-RPC 2.0 with LSP-style
`Content-Length` framing on stdin and stdout, for editors and long-running programs.
It offers the same methods and results as the command line, sends library-change
notifications, honours `$/cancelRequest` and writes nothing else to stdout. Errors are
JSON-RPC errors whose `data.code` is the `error.code` above. Requests run in order
behind a queue of eight; a long request delays the ones after it.

## AI agents (MCP)

`ibis mcp` serves the Model Context Protocol over stdio (revision 2025-06-18) with four
read-only tools: `search`, `show`, `cite` and `styles`. Add it to an agent as a local
stdio server with the libraries it may read, for example:

```sh
# Claude Code
claude mcp add ibis -- ibis mcp --file /path/to/refs.bib

# Codex: ~/.codex/config.toml
# [mcp_servers.ibis]
# command = "ibis"
# args = ["mcp", "--zotero"]
```

Agents see bibliographic fields and citations only: never notes, annotations, source
paths, file locations or local links. Long values are shortened and each answer is at
most 256 KiB. Nothing connects beyond your computer; reading a running Zotero uses
its local API on `127.0.0.1` only.

Agents that run shell commands can also use the command line directly with `--json`;
the envelope, error codes and exit codes above are the contract.
