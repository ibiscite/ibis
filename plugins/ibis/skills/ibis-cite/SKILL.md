---
name: ibis-cite
description: Find references in the user's own library (Zotero, BibTeX, BibLaTeX or CSL-JSON) and write citations or bibliography entries from real records with Ibis. Use when the user asks to cite, reference or look up a paper or book, add a citation key, or format a reference in a style such as APA, Chicago, IEEE or MLA.
license: AGPL-3.0-or-later
compatibility: Needs a connected Ibis MCP server and the ibis command (https://github.com/ibiscite/ibis) on PATH with support for --app-libraries. Works offline and reads libraries only.
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
- A search hit with `partial: true` matches only part of the query; `typo: true`
  means spelling was corrected. Offer those results as suggestions and ask the user
  to choose before citing them.
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

## If the MCP server is unavailable

Stop and explain that Ibis must be connected before you can search or cite. Ask the
user to install the current `ibis` command on PATH, choose a library in Ibis, and
start a new agent session. `ibis mcp --help` must list `--app-libraries`; a command
that rejects that option is too old for this plugin.

Do not fall back to reading library files or raw `ibis search` / `ibis show` JSON.
Those outputs can contain private notes and local paths that MCP deliberately
removes. Do not bypass that boundary by reading the app's settings yourself.

## Choosing the output

- Markdown for Pandoc or Quarto: `syntax: "pandoc"`. LaTeX with BibLaTeX:
  `syntax: "biblatex"`; with natbib: `syntax: "natbib"`. Typst: `syntax: "typst"`.
- Writing prose or a document without a citation processor: a `style`, plus
  `form: "bibliography-only"` for the reference list.
- Match what the document already uses before choosing one yourself.
