# 0058 Interface language

Decided 2026-11-18 with German and Arabic, the 0.4 order's first item
([ROADMAP.md](../../ROADMAP.md)).

## Decision

- **One catalogue per language.** `locales/<tag>.ftl` holds every message in
  `en-US.ftl`, with the same variables, references and selector keys; a count
  says each plural form its language has (Arabic six). A test fails on a
  missing, extra or unknown message (`apps/desktop/src/i18n/locales.test.ts`),
  and the pseudolocale test fails on any word the window shows that did not
  come from the catalogue (`hardcoded.test.tsx`).
- **One owner.** Rust's `catalogue::Language` names the languages; the app
  resolves the language once at start (the Settings choice, else the first of
  the system's preferred languages Ibis speaks, else English) and every window,
  the menu bar menu and the native dialogs speak it. A new choice applies the
  next time Ibis opens. The command line and MCP stay English.
- **Citations keep their own language.** The style's or locale's language
  decides a citation and a bibliography; what is copied or pasted never depends
  on the interface language (`languages.test.tsx`).
  Format names that are words (Keys only, Chicago (notes)) are translated in
  the one output presentation (`format/outputs.ts`); proper names such as
  Pandoc or APA are not.
- **Settings outlive languages.** A language value this build does not speak,
  from another build, follows the system without invalidating the settings
  file.
- **Left and right are on screen.** Move Left and Move Right, by key or from
  More, go through one shelf action that reads the citation line's direction.
- **Right to left by `dir` alone.** A right-to-left language sets `dir="rtl"`
  on the window; layout uses logical properties, and the few motions CSS has no
  logical form for multiply by `--inline-sign` (`tokens.css`). Glyphs that point
  back or forward mirror (`ui/Icon`); left and right arrows do not.
- **Mixed direction stays readable.** A translation isolates each argument
  (Fluent's isolation marks), and names, titles, containers and library names
  render with `dir="auto"` or `<bdi>`, so a Latin title, name, year, key, DOI or
  page range keeps its order in an Arabic sentence. English text and every Rust
  output (terminals, agents, native menus) carry no isolation marks. Digits stay
  Latin (`ar-u-nu-latn`), as in citations, keys and page numbers.
- **Long words fit.** The canvas draws the picker, setup and Settings in each
  language, in light and dark, and the layout guards measure every slot's
  messages in every language at the narrowest window.
