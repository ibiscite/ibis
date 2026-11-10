# 0007 Formatting (with 0028)

Formatted citations use Zotero's maintained build of citeproc-js, pinned by
commit and hash, running in embedded QuickJS with ICU4X collation inside
`ibis-csl`. All official CSL styles and locales ship by 0.4. Key outputs
(Pandoc, BibLaTeX, LaTeX, Typst and others) come from `ibis-core`, escaped per
syntax, with a warning when a syntax cannot say something. Outside a document,
Ibis copies only what one citation can get right: numeric styles copy the
entry without its number, note styles the full first note (never ibid.), and
generated author-date year suffixes wait for a document-level view. Source-supplied
literal dates and year letters are retained as data (2026-10-30, #1519).
Literal years retain numeric date order independently of their rendered letters.
Ordinary ALM/LRM/RLM direction marks retain their text meaning; embedding,
override and isolate controls are neutralized before output (2026-10-30, #1522).
BibTeX imports retain stored title capitalization, including unprotected proper
nouns. Styles and citeproc own presentation; APA uses the pinned processor's
capitalization at explicit subtitle boundaries, preserving abbreviation periods
and protected spans, and reports that titles need sentence case. Fixed-code,
item-indexed citation warnings carry source conversion uncertainty without field
text or paths through engine, MCP, desktop preview and copy feedback; that
uncertainty participates in citation fingerprints. MCP omits `warnings` when
empty, preserving clean response shapes; warned responses include the same
codes as the engine and command line
(2026-10-31, #1530).

2026-10-31 (#1486): The shelf consumes engine-owned reference spans. Form
examples and support reasons are read-only alternatives against the prepared
ticket's immutable snapshot and original output family; they never replace its
copy ticket. Year only uses the engine's year-only form. Late previews and
detail replies cannot publish into a newer edit or picker presentation.
Output-only display leaves preserve passive emphasis, scientific scripts,
small caps and underline; adding a reference or confirming Copied cannot flatten
them. The citation request's affix vocabulary is unchanged. Full text is
selectable in the reference menu as well as the tooltip, including for one
long reference.

2026-11-02 (#19): every official CSL style and locale ships in the binary as one
compressed bundle (`crates/ibis-csl/vendor/csl-bundle.bin`), built from tagged
upstream releases pinned by commit, so styles change only with an Ibis release.
Each file stays byte for byte upstream's, under its CC BY-SA 3.0 licence and with
its authors. Files sit in 1 MiB zstd blocks: a request unpacks only what it names,
and nothing stays in memory but the last style used. A dependent style runs its
parent's rules in its own default language. What a style does comes from what it
declares (its class, its citation number, its year suffixes, its conjunction),
never from its id; upstream's renames keep old ids working. A result names its
language, which is the request's, else the style's, and never the interface's or
the operating system's. An unknown language is refused rather than approximated,
except that a bare language means its main dialect, as upstream's `locales.json`
defines it.

2026-11-10 (#19): the credit for styles is the Citation Style Language project's
and each style's own. `styles` with an `id` returns that style's authors,
contributors, licence and documentation links as its `<info>` gives them, a
dependent style's own rather than its parent's; emails stay in the file.
`ibis styles --about [ID]` shows it, and the app's About view will use the same.

2026-11-10 (#19, design attacked by an independent review before code): your
styles. Keep this version and any `.csl` file live in one folder,
`com.ibiscite.ibis/styles/` beside the stable app's settings, as `<id>.csl`,
shared by Ibis, Ibis Nightly and `ibis`; front ends name the folder, so each
test has its own.
- A style there wins over the bundled style of that id, before upstream's
  renames, and as the parent of a bundled dependent style too. No Ibis release
  changes it; the list marks it, with the bundled style's date when that differs.
- Keep copies the bundled file byte for byte, with a dependent style's parent
  unless you have one; keeping over a different version of yours is refused.
  Add takes a file of at most 1 MiB whose `<id>` ends in a plain id, whose
  parent and language exist, and that writes a citation of an invented
  reference within the usual limits; it replaces yours of that id, saying so.
  Remove deletes yours and names your styles that used its rules.
- Your files are outside input, read when used: a regular file under its exact
  name, its `<id>` agreeing with that name, its parent an independent style
  other than itself, its language a bundled one. Otherwise the citation is
  refused, never written by the bundled style of the same id. Every cache
  follows the files' contents, not their ids.
- Author only, year only and narrative follow the SHA-256 of the rules checked
  against Zotero (`VERIFIED`), so a kept copy of a checked version keeps them
  and a new upstream version has them once checked again.
- Agents cite with your styles and see them listed; only a person changes them.
