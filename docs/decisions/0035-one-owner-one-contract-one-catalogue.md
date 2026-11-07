# 0035 One owner per picker session, one contract, one catalogue

A review of main on 2026-11-04 traced a week of repeated picker fixes (#1403,
#1558, #1093, #1179, #1603, #1642) to one cause: a picker session's results,
preview, destination and presentation have separate async owners on both sides
of IPC, joined afterwards by comparing about fifteen counters, with React effects
as the scheduler. The same review found five output vocabularies with lossy
conversions, four record summaries that disagree, three wordings of each error,
and app types generated from the app's own structures rather than the engine
schema. Before 1.0 these are replaced, not patched (maintainer, 2026-11-04,
"full freedom to restructure"):

- **One session owner.** Each picker session is a pure Rust state machine
  (`(state, event) -> (state, effects)`); every effect carries its session and a
  sequence number, and only the current one may complete. The web view sends
  intents and renders a generated view snapshot; it holds no async state of its
  own. Built in stages under #1260.
- **One contract.** The app speaks the engine's `CitationRequest`, `OutputSpec`
  and `ItemRef`; app-only output types and their conversions are deleted; app
  types come from the engine schema. A format, style or form has one name, from
  the engine, in every front end.
  The engine's `outputs` list is that one vocabulary (maintainer, 2026-11-04):
  its ids are the command line's 17 format names plus `pandoc-bare`; a common
  style's one name is a short name kept in the CSL manifest ("APA", "Chicago
  (notes)", at most 24 characters), and its full title is only the quiet
  detail beside it; `pandoc` stays the default output and `apa` the default
  style for finished text; CSL-JSON stays out of MCP; settings store one output
  id, and a formatted app's style comes from what Ibis remembers for that app,
  else the default style.
- **One catalogue.** The engine returns codes with arguments and one record
  summary; one Fluent catalogue words them for the command line, MCP and the app.
- **One platform seam.** Capture, delivery, selected text, clipboard, shortcut,
  login item and window presentation sit behind one trait with a fake, and the
  app shows what each system can do from a capability report, not from
  `cfg` branches spread through the code.
- **Tests prove outcomes.** Tests that parse source, workflow YAML or CSS with
  regular expressions, or that fix call order or English wording, are replaced by
  behaviour checks, generated race orderings with a replayable seed and golden
  data; time comes from an injected clock, never a sleep.

Lessons taken from Zed (fake platform, seeded executor), FFmpeg (libraries never
print, golden data reviewed as diffs), VLC 4 (one player object to end races),
Pandoc (one model between readers and writers) and Better BibTeX (editors pull
citations instead of synthetic paste).
