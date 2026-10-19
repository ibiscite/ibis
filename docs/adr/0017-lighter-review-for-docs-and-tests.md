# 0017. Lighter review for documentation and test changes

Status: accepted (2026-10-19); amends 0015

## Context
ADR 0015 requires an independent review of the exact final commit before an agent merges any pull request, and CI builds and exercises the native app on three systems for every change. For a solo maintainer this made a 200-line documentation change take about 40 minutes, most of it waiting, while the risk it guards against (a crash, data loss or a security hole) cannot come from prose. The maintainer approved a lighter process in #80 on 2026-10-19, keeping the full bar for code.

## Decision
- A pull request that changes only documentation (Markdown, `docs/`, issue templates), or only tests and synthetic fixtures without changing production code, may be merged by its agent author when all required checks pass and the author has done and recorded their own adversarial check. It needs no separate independent review.
- Everything else still needs the independent review from ADR 0015: production code, CI workflows, scripts, dependencies, settings, and ADRs other than ones recording an already agreed decision.
- When a reviewed pull request gets a small fix for review findings, the reviewer re-checks only the changed lines and records that on the pull request; a full new review is needed only when the fix is large or changes the design.
- CI skips the native app build and exercise when a pull request changes only documentation. The required `Desktop (...)` checks still run and report, so they are never left pending. Pushes to `main`, and any case CI cannot classify, build in full.

## Consequences
Documentation and test changes land in minutes, and reviewer time goes to code. A mistake in documentation is caught later rather than before merging, which is acceptable because it is easy to fix and cannot harm a user's library. If a change is wrongly treated as documentation-only, `main` still builds it in full on merge.
