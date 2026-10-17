# 0011. AI-assisted contributions with guardrails

Status: accepted (2026-10-17)

## Context
Many contributors, including non-programmers, use AI coding assistants. Review capacity is limited, so automated checks must carry most of the verification.

## Decision
AI-assisted changes are welcome when disclosed, small, accompanied by a failing-then-passing test or fixture, and green on all checks. `AGENTS.md` is the single instruction file for coding agents and contributors. Automated checks (types, lints, tests, snapshots, dependency policy) gate every change; a maintainer approves. Data contributions (profiles, templates, fixtures, translations) are preferred over code.

## Consequences
Quality depends on the strength of the test suite, so the fixture corpus is a first-class part of the project.
