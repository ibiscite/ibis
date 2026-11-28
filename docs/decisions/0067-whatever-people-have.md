# 0067 Ibis uses whatever people have, and needs nothing

**Decided** 2026-11-28 by the maintainer, answering direction questions while
the native apps were planned ([0066](0066-native-shells-use-system-parts.md)).
It builds on [0056](0056-open-layer-for-research-material.md) and changes none of
its limits.

## The decision

1. **Nothing is required; everything present is used fully.** Ibis works on
   its own, with nothing else installed: a `.bib`, CSL JSON, RIS or CSV file
   or a folder of PDFs is a complete setup, and Ibis can add to it. With Zotero,
   Better BibTeX or another manager present, Ibis takes full advantage of it
   (live library, collections, exact selection, its document fields) without
   asking the person to change how they work. Each adapter declares what its
   manager can do, and the app offers only that. Ibis is still not a reference
   manager or a sync service (ROADMAP › Do not build).
2. **Word citations as close to Zotero's and as robust as possible.** Most of
   Ibis's first writers use Word or LaTeX, on Windows and Mac. The method is ours
   to choose by evidence: formatted paste and `ibis format` today, Zotero's own
   fields where the trial ([#1378](https://github.com/ibiscite/ibis/issues/1378))
   proves them robust, and fields or an add-in of Ibis's own only if they beat
   both on the coauthor round trip. The ROADMAP's line against Ibis's own Word
   fields stands until that evidence exists.
3. **AI is optional, with full freedom for those who want it.** Everything the
   app does is available through the CLI and MCP for agents and power users
   ([0055](0055-agent-layers-full-text-writes-remote.md)). Someone who wants no
   AI sees none of it: no prompts, panels or network use in the default flow.
4. **References live in people's own files**, offline and private. There is
   no Ibis sync service or account; files sync however the person already
   syncs them.
5. **Coauthors get both routes.** A document carries the references it cites
   (a `.bib` beside a LaTeX file, the data inside a Word or ODT file), so
   whoever opens it can keep citing. A team can also point Ibis at one shared
   reference file in a folder it already syncs (Dropbox, iCloud Drive, Git).
   The 1.0 coauthor round trip through Zotero's document transfer is the first
   case tested.
6. **Plugins are small programs over the CLI and MCP**, not an in-app plugin
   API or store ([0039](0039-extending-ibis-from-outside.md)).
7. **Settings are simple by default, with full depth for those who want it.**
   The design is still open: it goes to the maintainer as A/B mock-ups
   rather than a plain "Advanced" section.
8. **Updates are quiet and automatic**, with release notes one click away.
9. **Desktop first; a web version is planned after 1.0** for reading and
   citing, over the same engine. Nothing before 1.0 may block it.
10. **Every writing app is served equally**: Word, LaTeX editors, Google Docs,
   Pages, LibreOffice, Obsidian, Markdown and Typst editors. Each gets the
   best citation its document format can hold.
11. **The name is always Ibis**: the app on every system, the CLI and the
   brand. Working names such as "Ibis Native" are not used.

## Consequences

- First-run setup finds what is installed and starts from it; with nothing
  found, choosing or creating one file is the whole setup.
- Features that depend on a manager appear only when that manager is present,
  and say what they need instead of failing.
