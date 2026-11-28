# LaTeX manuscript evidence

What LaTeX makes of the BibTeX Ibis writes. Every invented item of the mapping
corpus (`../zotero/mapping`, 297 items over all 37 item types) is read two
ways: as a Zotero item with a citation key, and from a CSL-JSON file (Ibis with
no Zotero). Ibis writes each one as BibTeX (`ibis cite KEY --format bibtex`),
and one manuscript cites them all with biblatex and Biber under pdfLaTeX, as
Overleaf compiles by default, in two styles: biblatex's `authoryear` and
`biblatex-apa`. All data is invented.

`crates/ibis-cli/tests/latex_manuscripts.rs` compiles the manuscript and
requires that Biber finishes and that every entry reaches the typeset page with
its title. Everything else is listed in `biblatex-STYLE.json`, one finding a
line: a record Ibis refuses to write, a Biber warning, a LaTeX error during an
entry, or an entry whose title is not on the page. The test fails when a list
changes; review the change like a snapshot, then run
`IBIS_BLESS=1 cargo test -p ibis --test latex_manuscripts`.

It runs where TeX Live (pdfLaTeX, biblatex, biblatex-apa, Biber, Latin Modern)
and Poppler's `pdftotext` are installed, and is skipped elsewhere;
`IBIS_REQUIRE_TEX=1` makes them required. Hosted CI does not install TeX yet.
On Ubuntu:

```sh
sudo apt-get install --no-install-recommends texlive-latex-base texlive-latex-recommended \
  texlive-bibtex-extra biber lmodern poppler-utils
```

## What it shows (28 November 2026)

Recorded with TeX Live 2023, biblatex 3.19 and Biber 2.19. Ibis writes 593
entries (one record is refused on purpose: its two citation keys disagree).
Biber reads them all, and in both styles every entry reaches the page with its
title except one in Japanese. What remains:

- **Invented data.** Biber checks ISBN check digits, which the invented ISBNs
  do not have, and warns that dates Ibis keeps as written (`Spring 2020`, an
  impossible `2026-02-30`) are not years it can sort.
- **pdfLaTeX's own limits.** Japanese, Arabic, a lone combining accent and the
  letters ʻ and ʼ need packages or LuaLaTeX; pdfLaTeX stops on them.
- **Ibis.** An original date kept as text (`origdate = {Invented literal}`) is
  ignored by Biber, which reads only ISO dates there.

The first run found three faults in the BibTeX writer, fixed with it: a name
with a double quote and a particle made Biber run forever; years before the
common era were one year off, in reading and in writing, because biblatex
counts them as ISO 8601 does (0000 is 1 BC) and CSL as historians do; and an
ISBN with `_` stopped LaTeX, because biblatex typesets ISBNs as text.
