# Architecture Decision Records

One page per decision with lasting effect: context, decision, consequences. Status is *accepted*, *proposed* or *superseded by NNNN*. To change a decision, add a new ADR that supersedes the old one; do not rewrite history. ADRs whose features are not fully built yet (0006, 0007, 0009, 0010) record the direction for the unbuilt parts; those details are settled when the parts are built, and a new ADR can change them then. See also `../architecture.md` and `../../ROADMAP.md`.

| # | Decision | Status |
| --- | --- | --- |
| 0001 | Record decisions as ADRs | accepted |
| 0002 | License: AGPL-3.0-or-later; contributions under the DCO | accepted |
| 0003 | One engine, thin front ends | accepted |
| 0004 | Desktop shell: Tauri 2, engine and picker as separate processes | accepted (amended by 0012) |
| 0005 | Canonical data model: CSL-JSON plus raw source record | accepted |
| 0006 | Read-only sources; Zotero access ladder | accepted |
| 0007 | Citation engine and styles | accepted |
| 0008 | Offline by default; every network feature opt-in | accepted |
| 0009 | Insertion never fails silently; destination behaviour is data | accepted |
| 0010 | Search: instant keyword first, meaning-based search added later | proposed (awaiting measurements) |
| 0011 | AI-assisted contributions with guardrails | accepted (amended by 0015) |
| 0012 | Ship 0.1 as a single process; split only if measurements require it | accepted (amends 0004) |
| 0013 | Crate-specific NCSA exception for isolated fuzzing infrastructure | accepted |
| 0014 | Share one lockfile across application and fuzz infrastructure | accepted (amends 0013 workspace layout) |
| 0015 | Autonomous agents work under standing maintainer authorization | accepted (amends 0011; amended by 0017) |
| 0016 | Versioned settings in the platform configuration folder | accepted (library field and migration superseded by 0018) |
| 0017 | Lighter review for documentation and test changes | accepted (amends 0015) |
| 0018 | Settings format 2: the library names its kind | accepted (supersedes part of 0016) |
| 0019 | Signed in-app updates from GitHub Releases, with separate Stable and Nightly apps | accepted (applies 0008) |
