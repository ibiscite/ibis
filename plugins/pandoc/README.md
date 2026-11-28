# Ibis for Pandoc and Quarto

A Lua filter that fills in the references a document cites from the libraries
Ibis reads, Zotero included, so Pandoc's citeproc formats them. You write
`[@vale2024lanterns, p. 12]` as usual and need no exported `.bib`.

It needs the `ibis` command ([install](../../docs/cli.md#install)) and Pandoc
3.1.1 or later, which Quarto 1.4 and later include.

## Pandoc

Download [`ibis.lua`](_extensions/ibis/ibis.lua) and run it before citeproc:

```sh
pandoc paper.md --lua-filter ibis.lua --citeproc -o paper.docx
```

## Quarto

```sh
quarto add ibiscite/ibis/plugins/pandoc
```

Then name the filter in the document or `_quarto.yml`:

```yaml
filters:
  - ibis
```

## Which libraries

With no options it reads the libraries chosen in the Ibis app. The front
matter can name others, as `ibis --file` and `ibis --zotero` do:

```yaml
ibis:
  file: [thesis.bib]
  zotero: true
```

Set `IBIS` to the command's full path when it is not on the PATH Pandoc sees.

## What it does

- It adds only keys that the document's own `references` and `bibliography`
  do not hold, so a project `.bib` always wins.
- Records come from `ibis cite --format csl-json`, the same filtered export
  every Ibis client uses: notes and file paths never reach the document.
- A key Ibis does not hold is left to citeproc, which warns about it; Quarto's
  cross-references such as `@fig-plot` are left alone. A key that differs only
  in letter case gets a hint.
- `nocite: '@*'` keeps meaning the document's own bibliography; it never pulls
  in a whole library.
- Numbering, ibid. and year letters stay with citeproc, which sees the whole
  document.

Nothing is written: the references exist only in Pandoc's run.
