---
name: ibis-cite
description: Find references in the user's own library (Zotero, BibTeX, BibLaTeX or CSL-JSON) and write citations or bibliography entries from real records with Ibis. Use when the user asks to cite, reference or look up a paper or book, add a citation key, or format a reference in a style such as APA, Chicago, IEEE or MLA.
license: AGPL-3.0-or-later
compatibility: Needs the ibis command (https://github.com/ibiscite/ibis) on PATH, or its MCP server. Works offline and reads libraries only.
metadata:
  author: ibiscite
  homepage: https://github.com/ibiscite/ibis/blob/main/docs/cli.md
---

# Cite from the user's library with Ibis

Ibis reads the user's reference library and formats citations from the records it
holds. You find the reference; Ibis supplies the facts. That way every citation you
write is a record the user actually has.

## Rules

- Never write a citation, key, DOI, author list or year from memory. Cite only items
  that Ibis returned in this conversation, by the key or id it returned.
- If Ibis finds nothing, say so and show what you searched for. Do not guess a near
  match: offer the closest results and let the user choose.
- When a result lists `notices`, `losses` or item warnings, tell the user what needs
  checking. A refused item (changed, missing or ambiguous) is not cited; search again.
- Ibis is read-only. It cannot add or change references; ask the user to do that in
  their reference manager.

## With the MCP tools

If the `ibis` MCP server is connected, use its four read-only tools:

1. `search` with a few distinctive words: an author surname, title words, a year or a
   key. Results are short; refine the words rather than asking for many results.
2. `show` with an item from `search` when you need more fields (container, pages, DOI).
3. `cite` with the items, in order, and either a `style` (default `apa`; ids from
   `styles`) or a key `syntax` (`pandoc`, `biblatex`, `natbib`, `latex`, `typst` or
   `keys`) that matches the document being written.
4. `styles` lists the bundled styles.

## With the command line

If the MCP server is not connected but you can run commands, use `ibis` with `--json`
and branch on `error.code`, never on the wording. With no library named, `ibis` reads
the one chosen in the Ibis app. To read another, name it with `--file PATH`, `--zotero`
or `--zotero-group ID`, for example the `.bib` file of the project being written.

```sh
ibis search "garcia 2021 memory" --limit 5 --json
ibis show garcia2021 --json
ibis cite garcia2021 --format pandoc             # [@garcia2021]
ibis cite garcia2021 --style apa                 # (García, 2021)
ibis cite garcia2021 --style apa --bibliography  # the reference entry
ibis cite garcia2021 --locator "p. 12" --prefix see lee2019 --format autocite
ibis search "garcia" --file thesis.bib --json    # a named library instead
```

Exit code 0 is success (an empty search is a success), 1 means done with findings to
report, 2 a wrong command line or no library to read, 4 a key that is not there and 5
a library that cannot be read right now. Pass on the `ibis:` sentence from stderr when
a command fails; it says what to fix. `ibis --help`, `ibis <command> --help` and
`ibis schema <command>` describe everything else.

## Choosing the output

- Markdown for Pandoc or Quarto: `pandoc`. LaTeX with BibLaTeX: `biblatex` in MCP, or
  `--format autocite`, `parencite` or `textcite` on the command line; with natbib:
  `natbib`. Typst: `typst`. `ibis cite --help` lists every command-line format.
- Writing prose or a document without a citation processor: a `--style`, plus
  `--bibliography` for the reference list.
- Match what the document already uses before choosing one yourself.
