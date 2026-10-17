# 0001. Record decisions as ADRs

Status: accepted (2026-10-17)

## Context
Contributors need short, findable records of decisions that are expensive to reverse, including the reasoning and the alternatives considered.

## Decision
Use one-page ADRs in `docs/adr/` for one-way decisions: license, persisted data formats, public CLI/API contracts, network use, security boundaries, platform strategy. Two-way decisions (internal structure, UI details, library choices that are easy to swap) are made in the pull request and need at most a day of investigation.

## Consequences
Decisions are changed only by a superseding ADR. `ROADMAP.md` and `docs/architecture.md` summarize the current state; ADRs take precedence where they differ.
