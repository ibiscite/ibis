# 0027. Move fast until 1.0: no compatibility code, review by risk

Status: accepted (2026-10-26); amends 0015 and 0017; suspends the upgrade rules of 0016, 0018, 0022 and 0023 until 1.0

## Context

Ibis has only preview testers, who accept losing settings between pre-1.0 versions. Its maintainer works alone through several AI agents in parallel and asked, on 2026-10-26, for a process that keeps only what prevents real damage and lets the product be rebuilt freely until 1.0. Two rules cost the most for the least protection at this stage:

- Compatibility code. About 1,800 lines (`identity.rs`, the settings format 1 to 2 upgrade, `.bak` handling) exist only to carry settings from older versions. Every settings change has to extend them.
- Review of every code change. ADR 0015 requires an independent review of the exact head for every code change, and the loop added a second-model attack before the pull request and a routine bot review on top. Most changes are low risk and already covered by tests and CI.

## Decision

**No backward compatibility before 1.0.** Settings, other persisted files, the command line and internal formats may change without migrations, upgrade paths or compatibility shims. When Ibis finds settings it cannot read, it keeps the old file as a copy, starts from defaults and says so in plain words. Saves stay atomic and flushed, and Ibis never silently overwrites a file it could not read. Suspended until 1.0: format upgrades (0016, 0018), preserving unknown fields (0022) and carrying settings to a new app identity (0023). Existing migration code, including the app identifier move in #695, keeps working until its area is next rebuilt and is then deleted rather than extended. From 1.0, these ADRs describe the baseline again: a persisted format change then needs an upgrade path.

**One review, only where risk is real.** An independent review of the final head (by a session that did not write the change, preferably another model) is required for changes to:

- the updater, release signing and CI permissions;
- settings and other files Ibis writes;
- the clipboard, escaping and pasting into other apps;
- window permissions, native input and the shortcut portal;
- BibTeX and Zotero reading, citation syntaxes and formatting;
- anything that handles untrusted input, network access or security boundaries.

It replaces the separate pre-pull-request attack and the routine bot review. All other changes merge when the required checks pass and the author has recorded their own adversarial check (the rule ADR 0017 set for documentation and tests). Every P1 and P2 finding is still fixed before merging.

Everything that prevents real damage stays: the required "CI result" on the exact head, no stacked branches, one writer per set of files, tests for every fix, revert first when a real failure turns `main` red, privacy rules, licence checks, signed-off commits with public identities, and no bypasses.

## Consequences

Agents spend their time on the product, and a settings or format change is a small edit instead of a migration. Someone running a pre-1.0 build may lose saved preferences after an update; Ibis says so and keeps the old file. Mistakes in low-risk code are caught by tests, CI and use rather than before merging; if a class of mistake recurs, its area joins the review list or gets a check.
