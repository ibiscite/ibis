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
2. **Word citations as close to Zotero's and as robust as possible.** Word is
   where most of Ibis's first writers work, beside LaTeX. The method is ours
   to choose by evidence: formatted paste and `ibis format` today, Zotero's own
   fields where the trial ([#1378](https://github.com/ibiscite/ibis/issues/1378))
   proves them robust, and fields or an add-in of Ibis's own only if they beat
   both on the coauthor round trip. The ROADMAP's line against Ibis's own Word
   fields stands until that evidence exists.
3. **AI is optional, with full freedom for those who want it.** Everything the
   app does is available through the CLI and MCP for agents and power users
   ([0055](0055-agent-layers-full-text-writes-remote.md)). Someone who wants no
   AI sees none of it: no prompts, panels or network use in the default flow.
4. **Coauthors are an open question.** Shared libraries, two people citing in
   one manuscript and handing a document between Word and LaTeX writers need
   their own design before 1.0; the 1.0 coauthor round trip through Zotero's
   document transfer is the first case.
5. **The name is always Ibis**: the app on every system, the CLI and the
   brand. Working names such as "Ibis Native" are not used.

## Consequences

- First-run setup finds what is installed and starts from it; with nothing
  found, choosing or creating one file is the whole setup.
- Features that depend on a manager appear only when that manager is present,
  and say what they need instead of failing.
