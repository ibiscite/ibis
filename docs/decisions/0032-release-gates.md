# 0032 Release gates

From 0.3 a release ships only when the whole citation (search, choose, page,
paste or copy as that platform supports, back in the document) works with
VoiceOver and NVDA and with Chinese,
Japanese and Korean input methods; Orca joins for 1.0. Interface text goes
through the translation layer from 0.3, with German and one right-to-left
language in 0.4. The speed and resource budgets in
[architecture](../architecture.md#desktop-app) are limits: search and bundle
checks fail pull request CI; checks with too little margin on shared runners
report there and are enforced on the release machines before each release.
The reader inside Ibis no longer waits for citing from Zotero's annotations
(maintainer, 2026-11-04, replacing the 2026-10-28 order; see 0036).

2026-10-31 (maintainer): limits come from what a person notices, at the size real
libraries reach. They block a release at 10,000 generated realistic records
(abstracts, long author lists, Chinese, Japanese and Korean text): shortcut to a
typeable picker p95 ≤ 100 ms; keystroke to results p95 ≤ 50 ms (engine search
≤ 20 ms); warm formatted preview p95 ≤ 100 ms and the first after launch ≤ 1 s;
first results ≤ 1 s after launch; citable again about 2 s after a source edit, with
search never interrupted; hidden whole-app memory ≤ 200 MiB, web view included;
hidden idle ≤ 0.2 % of one core and ≤ 1 wakeup a second; picker JS and CSS ≤ 400 KiB
gzipped. A 50,000-record check reports on nightly against the same numbers as a stretch
goal and must not grow faster than the library; a miss is studied, not blocking. The 100k limits and the 400-entry bibliography timing are dropped; document
tools set their own later. Where each check runs: #1536.

2026-11-01 (maintainer): every planned feature ships before 1.0, including ideas once
listed after it; the version numbers before 1.0 are working steps and may be
renumbered. 1.0 adds no features: people test everything and the maintainer
moves to 1.0 by hand. Until signing is decided, macOS keeps the stable
self-signed identity and Windows installers stay unsigned with first-launch
steps ([ROADMAP](../../ROADMAP.md)).

2026-11-02 (maintainer): every planned milestone through 0.7 stays in 1.0. The
PDF opens in a panel inside the picker that widens, shared with the citation
editor. Delights are quiet, with rare moments. Approved cuts: two accents only, a
trimmed Look page, and one format memory rule. First run asks first, with Zotero
highlighted when found. Agents may use the Windows laptop over ssh and a Linux VM
on the Mac for release checks, not only for bugs. Once 0.3 is released it is
announced publicly to open early adopters, with Report a Problem collecting
feedback. Paid Mac signing is undecided: it is revisited near 1.0 and stays an
open question for the 1.0 gate.

2026-11-02 (maintainer): both channels stay. Each finished milestone (0.3, 0.4 and
so on) becomes one numbered release with the maintainer's one-click approval. The
numbered release is the roughly stable version for early adopters, and every
distribution stays on it, including the Claude plugin, Homebrew and the command
line. Nightly is where daily testing happens.
