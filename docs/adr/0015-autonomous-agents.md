# 0015. Autonomous agents merge only small, reviewed, low-risk changes

Status: accepted (2026-10-18); amends 0011

## Context
ADR 0011 says a maintainer approves every AI-assisted change. In practice the maintainer runs coding agents that work for hours unattended, and requiring approval of every change would stall them. The first day of unattended work showed the opposite risk: 26 pull requests merged with a median of 8 minutes from opening to merge, half of them over the size guideline, with review claimed only in the author's own words, and infrastructure done before the milestone's user-facing work. The maintainer could not yet follow or fix the resulting code without an agent.

## Decision
Implementing agents may merge their own pull requests only when the change is small and low-risk (bug fixes, tests, fixtures and documentation corrections of about 300 changed lines at most, adding no feature and changing no interface, default, persisted format or file that defines checks and process), all required checks pass, and an independent review of the final commit is recorded on the pull request with every P1 and P2 finding fixed. Every other change waits for review by the maintainer or by a reviewing agent acting for the maintainer, which also orders the work in the milestone's tracking issue. Agent work is disclosed in commits and pull requests. Releases, tags, settings and product defaults stay with the maintainer. The exact rules are in `AGENTS.md`.

## Consequences
Low-risk work keeps moving while the maintainer is away; changes users would notice get a second look before they land. Reviews become verifiable on GitHub. Pull requests carry a plain-language explanation so the maintainer can learn the code over time. The rules are enforced by instruction rather than repository settings; if they are not followed, the maintainer tightens the repository rules (required approvals) instead.
