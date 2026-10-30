# Terminal kit

The [`ibis` CLI](../../README.md#command-line) runs independently of the desktop
app. These tools use the same public commands and citation checks.

## Pick with fzf

With `ibis`, `jq`, `fzf` and Bash on PATH, run the included
[`ibis-fzf` recipe](recipes/ibis-fzf):

```sh
bash crates/ibis-cli/recipes/ibis-fzf references.bib 'garcia'
```

The first argument is a BibTeX/BibLaTeX or CSL-JSON file; the optional second
argument is the initial Ibis search. It searches through `ibis search --json`,
then lets fzf filter those results. Tab selects several references; Enter prints
one Pandoc citation. Escape or a failed fzf invocation prints no citation.
It returns at most 1,000 search hits: narrow the initial search for a larger
library. Source warnings or a failed search stop the recipe before selection;
inspect them with `ibis libraries --file references.bib --json`.

The hidden first column carries the complete picked object, including its
`ItemRef` and cite fingerprint. The visible title marks invisible direction/C1
controls with `[control]` and is JSON quoted to escape newlines and other
terminal controls. The recipe builds a canonical request and calls
`ibis cite --request`; it never resolves a citation key itself. A changed,
deleted or ambiguous selection is refused by the engine, with no citation on
stdout. Change `output` in the recipe's request to
`{kind: "csl", style: "apa"}` for formatted citations. The recipe writes no
temporary files and invokes only `search` and `cite` on Ibis.

## Shell completions

Generate completions directly from the installed binary's Clap command tree:

```sh
ibis completions bash
ibis completions zsh
ibis completions fish
```

For the current shell session:

```sh
# Bash
source <(ibis completions bash)

# zsh, after completion is initialized
autoload -Uz compinit && compinit
source <(ibis completions zsh)

# fish
ibis completions fish | source
```

Add the corresponding line to your shell's startup file to load it next time,
or save the generated script and source it. Regenerate saved scripts after an
Ibis update. Completion covers command names, flags, finite argument values
(such as citation formats) and file arguments. It reads no libraries and does
not complete reference titles or keys. `--json` wraps the generated text in
`data: {shell, script}` instead of printing raw shell code.

## Offline diagnosis

```sh
ibis doctor --file references.bib
ibis doctor --file references.bib --file other.json --json
ibis doctor --config settings.json --file references.bib --json
```

Doctor reads and reports only: no writes, source watches, API requests, database
copies, telemetry or repairs. It checks only files you name. `--config` validates
the chosen Ibis settings file through the shared settings reader, including
backup provenance; it does not load the libraries named inside that config or
replace a broken primary with its backup. Without `--config`, config is explicitly
`not-checked`. Without a selected library, doctor reports `no-library`.

`--zotero` and `--zotero-group` report `zotero-not-checked` with unknown freshness.
The existing Zotero loader contacts its local API and may create a private
database copy, so doctor cannot use it under its no-send/no-write guarantee.
This result makes no claim about whether Zotero is running. To diagnose an
offline export, give it with `--file`. To load Zotero through the usual reader,
use `ibis libraries --zotero --json` separately.

`doctor --json` returns the usual `{"ibis":1,"ok":true,"data":…}` envelope
when the report completes, including when checks find problems. `data` includes:

- `contract_version`, `cli_version`, and `stdout_tty` (no terminal is required).
- `capabilities`: engine method names, bundled `csl_styles` and supported key
  outputs/forms (including `single_item_forms` restrictions), generated from
  the engine registries.
- `config` and `library_check`: each has `status`, a stable `code` and a fixed
  explanatory `message`.
- `sources`: one independent result per selected source, in argument order
  (file choices first, then personal Zotero and groups). `index` identifies the
  check without printing a path. Successful reads include the opaque library
  `id`, `warning_count` and `refresh_error`; no titles, keys, contents, warning
  payloads or source paths appear in either output mode.
- Each source's `freshness` has `state` and `updated_at_ms`, the Unix-epoch time
  of the last successful engine read. `current` means this invocation read it
  successfully, not that an export is up to date with Zotero. `stale` means a
  retained snapshot whose refresh failed; `unknown` has a null timestamp and
  never borrows another source's success time. Checks run serially, and a later
  failed check cannot relabel an earlier one. Each invocation reads files anew.

Check codes are stable; messages may improve:

| Check | Codes |
| --- | --- |
| Config | `config-ok`, `config-not-requested`, `config-missing`, `config-unreadable`, `config-invalid`, `config-newer-format` |
| Selection | `no-library`, `sources-reported` |
| Source | `source-ok`, `source-warnings`, `refresh-failed`, `library-missing`, `library-unavailable`, `library-not-regular`, `library-invalid`, `resource-limit`, `newer-format`, `internal`, `zotero-not-checked` |

Doctor exits **0** for a completed report with healthy selected sources, or **1**
if any requested check fails, has source warnings or cannot be checked, or if no
library was selected. An unrequested config check does not cause exit 1.
Invalid arguments exit **2** with the usual `usage` error envelope under
`--json`. The shared response-size limit exits **5** with `resource-limit` if
the report itself is too large; internal/transport failures exit **70**.
Individual source failures remain in the report with exit 1, even when ordinary
`search` or `libraries` would exit 3, 5 or 6 for that same source.
