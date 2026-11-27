# 0063 Form follows the job: typed citations, one home for each choice and each message

**The maintainer's decision**, 2026-11-26, answering a first-principles review of
the picker ([#2014](https://github.com/ibiscite/ibis/issues/2014)): "let form
follow function, do not worry about what was done in the past". Ibis does one
job: in any app, summon it, find the work, build the citation, deliver it, and
recover if anything went wrong. Each part of the interface exists for one step
of that job, and each choice and each message has exactly one home.
0062 says how parts are
built; this says which parts exist.

## The decisions

1. **A citation can be typed whole in the search field.** (Amended
   2026-11-26 by the maintainer, after an outside critique: the typed form is
   read **only when the person asks**, with one explicit action such as a key,
   as the CLI's `--shorthand` does. Ordinary search stays plain text, so titles
   with numbers and punctuation, German and Arabic locators, and input-method
   composition are never misread. It may become automatic later if people ask.)
   - The search understands locators (`p12`, `pp. 12–15`, `ch 3`, `sec 2.1`
     and the other CSL locator terms, in the interface language and in
     English).
   - `;` separates works, so `vale 2024 p12; ostrom ch3` builds a two-work
     citation. Each finished segment becomes a token, as it does when picked.
   - The engine parses this (`ibis-core`), so the picker, `ibis cite` and the
     MCP tools read the same grammar.
   - The drawer stays for rarer options only: prefix, suffix and form. The
     locator menus remain there for people who prefer them.
2. **A format is chosen and remembered only where people cite.**
   - The picker's format control and its palette are the one place: for this
     paste, this app or site, or a project.
   - Settings › Citations goes away. Its default formats become the palette's
     "for apps like this" choice. Its privacy rows (what AI apps may read)
     move to AI Apps, and editor connection moves to General.
   - The engine's format rule
     ([0042](0042-formats-apps-and-writings.md)) is unchanged; only its
     controls move.
3. **An empty search shows what you cited.**
   - Opening Ibis with nothing typed lists the works cited since Ibis opened,
     then recent ones. Copy Bibliography, Save and Reset List are commands in
     ⌘K and act on that list.
   - The separate Cited Since Opening sheet goes away. Its rules (what Ibis
     knows, record identities in memory only, undoable removal) move with
     the list.
4. **Every status and problem has one home: the list's heading line.**
   - Library loading, an unreadable or missing library, Zotero closed, paste
     not allowed, a refused format, an update: each shows there as one line
     with at most one fix as a button. Typed messages from the engine name
     the fix.
   - The delivery bar shows the citation, the library button, its delivery
     action and More (maintainer, 2026-11-26). The library button shows
     the source in its menu and tooltip. The empty list can give a
     longer explanation and a next action.

## Consequences

- **Engine and interfaces:**
  - The engine gains a citation-text grammar, with conformance tests shared
    by the desktop, CLI and MCP.
  - Status becomes a typed list the engine returns with every search, so no
    interface invents its own messages.
- **Design pages to update:** `picker.md`, `delivery.md`,
  `sources-and-sheets.md` and `settings.md`. Each change lands with its
  implementation, in this order:
  1. status line;
  2. empty-search history;
  3. formats in the picker only;
  4. typed citations.
- **Settings:** shrinks to General, Appearance, AI Apps and Shortcuts, plus
  About.
