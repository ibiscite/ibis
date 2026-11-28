---
name: review
description: Review an Ibis diff or pull request for bugs, broken rules and bloat, once, and report P1-P3 findings. Use before landing major work, for the independent review that guarded paths need, or when asked to review.
---

# Review a change

One light pass over the final head. Report what is wrong; do not redesign.

## Steps

1. Pin the head: `git fetch origin main` and `git diff origin/main...HEAD`
   (or the PR's head SHA). Read the PR text and the issue it closes.
2. Read only the rules the diff touches: [AGENTS.md](../../../AGENTS.md),
   ROADMAP.md › How we build, and the docs page for the changed part.
3. Look for, in this order:
   - **Correctness:** wrong output, lost data, a broken contract or client,
     a race, a privacy leak, a native effect without a deadline.
   - **Owner:** logic in a front end that the engine owns, a second way to do
     a job that already has one, a fix copied to a second place instead of
     the shared piece changed.
   - **Bloat:** compatibility code before 1.0, defensive guards for cases
     nobody has met, comments that justify a patch, dead code, tests of call
     order or source text, what the change replaced but did not delete.
   - **Explanation:** a visible change without its docs or website text.
4. Trace each finding end to end before reporting it. Drop any you cannot back
   with a file, a line and a concrete failure.

## Report

- **P1:** wrong citation, data loss, security or privacy break; must fix.
- **P2:** a real bug or broken rule a person or client would hit; must fix.
- **P3:** worth fixing, not blocking.

List the findings most severe first, each with the file, the line and the
failure. Fix P1 and P2 without adding guards for anything else. For paths that
`land.sh` guards, the reviewer itself (a session or subagent that did not write
the change) posts the verdict when none remain; the author never does:

```sh
gh api -X POST repos/ibiscite/ibis/issues/PR/comments \
  -f body="Independent review: no P1 or P2 at FULL_HEAD_SHA"
```

For a Sol review, the subagent that runs Codex posts Sol's verdict this way.
The author then requests squash auto-merge, which the maintainer has approved
for their own branches.
