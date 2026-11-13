# 0050 The menu bar menu

Decided by the maintainer on 2026-11-13 (direction A of three), after a study of
Itsycal, Dropzone, Ice, iStat Menus, Stats, Hidden Bar and Apple's guidance.

Ibis keeps a native menu, the same items on macOS, Windows and Linux. It adds:
- **Library switching in one click**, as checked items under a Library heading,
  with each problem reached through its fix ("Fix 2 Problems…").
- **Copy Again:** a submenu of the last five references cited in the frontmost
  app, in its remembered format, read when the menu opens. It copies and never
  pastes, because a menu cannot know where the caret is; the Copied notice
  confirms it. Closed libraries are left out, and Forget Recent Citations
  empties it.
- **A spoken state:** the bird tells VoiceOver what Ibis is doing ("Ibis,
  reading thesis.bib").
- **Text dropped on the bird searches for it**, and the bird raises its head for
  anything Ibis can read. A dropped PDF waits until the Reader reads PDFs, and
  its DOI lookup follows decision 0008.

Mac-only touches (⌥ alternates, subtitles) use objc2; muda gives the rest. Files
are watched, so there is still no Reload item.

Declined: a custom card instead of a menu, which goes against Apple's guidance
and design.md's native-menu rule, and cannot open on Linux. Never: text or
figures in the bar by default, a tinted bird, polling or animation while hidden,
pasting from the menu, permissions for flair, private layout tricks, or the bird
as the only way to reach anything.
