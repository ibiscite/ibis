# 0015. Autonomous agents work under standing maintainer authorization

Status: accepted (2026-10-18); amends 0011

## Context
ADR 0011 says a maintainer approves every AI-assisted change. In practice the maintainer runs coding agents that work for hours unattended, and requiring approval of every change would stall them. The first day of unattended work showed the opposite risk: 26 pull requests merged with a median of 8 minutes from opening to merge, half of them over the size guideline, with review claimed only in the author's own words, and infrastructure done before the milestone's user-facing work. The maintainer could not yet follow or fix the resulting code without an agent.

## Decision
The maintainer delegates engineering, security, testing, CI and documentation decisions to agents, including merging their own pull requests once all required checks pass on the final commit and an independent review of that exact commit is recorded with every P1 and P2 fixed. Agents may accept ADRs recording behaviour already agreed in the milestone tracking issue (#80 for 0.1). Product defaults users notice are chosen from evidence and stated in #80 so the maintainer can overrule them. Agents ask only about releases (tags, #70, publishing), licensing, new network use, repository or organization settings, costs or accounts, release scope, and work requiring the maintainer's own hands. Ask once in plain words, with a recommendation and a one-word answer; do not request authorization already given. Agent work remains disclosed in commits and pull requests. The exact rules are in `AGENTS.md`.

## Consequences
Engineering work keeps moving while the maintainer is away; independent review provides a second look before changes land, and the maintainer retains the reserved decisions. Reviews become verifiable on GitHub. Pull requests carry a plain-language explanation so the maintainer can learn the code over time. The rules are enforced by instruction rather than repository settings; if they are not followed, the maintainer tightens the repository rules (required approvals) instead.
