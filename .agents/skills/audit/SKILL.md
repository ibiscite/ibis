---
name: audit
description: Find what to delete or fix in an area of Ibis - drift between docs and code, duplicate ways of doing one job, stale text, dead code and low-value tests. Use when asked to audit, clean up or streamline, or before a milestone.
---

# Audit an area

The aim is subtraction: fewer ways, fewer words, fewer tests, same outcomes.

## Scope first

Take the area named. Otherwise find the hot spots: `git log --since=30.days
--name-only --format= origin/main | sort | uniq -c | sort -rn | head -30`.

## Look for

- **Drift:** a doc, the README or the website claiming what the code does not
  do, or a decision that a later decision superseded but that the text still
  follows.
- **Two ways:** two functions, components, scripts or settings doing one job.
  Apply the deletion test: deleting one should concentrate the logic, not move
  it.
- **Rules a structure could hold:** a prose rule or a policing check that a
  type, private visibility or generated code would make unnecessary.
- **Dead weight:** code with no caller, tests for withdrawn behaviour or call
  order, process text nobody follows, scripts no workflow or doc runs.

Check each candidate with `rg` for its callers and references before listing it.

## Report

A ranked list, highest payoff first, each with the evidence (paths, callers,
the contradicting line) and the smallest change that fixes it. Fix the clear
ones in one pull request; put the rest in one line each on the tracking issue
or the owning issue.
