# 0049 Local machines gate merges (superseded)

2026-11-15 (maintainer): superseded: hosted CI's fast tier is the required
merge check, no merge queue; macOS and Windows confirm on main and nightly;
local machines are for speed and native checks. Pull requests run Ubuntu only
unless `scripts/ci-scope.sh` finds platform, packaging, dependency or workflow
changes, or the PR has the `full-ci` label. They squash-merge once **CI result**
passes, without being up to date with main; main's push CI checks the combined
commit, and a red main holds the nightly.

2026-11-14 (maintainer): hosted **CI result** is the required check again. The
local gate reported on itself, and main went red the day it became required.
`scripts/land.sh` runs fast local checks, pushes the branch and requests
squash auto-merge.

2026-11-13 (maintainer): this is a solo project; the maintainer and his agents
are the only writers. **The local machines are the gate; GitHub CI confirms
after merge.** Waiting for a hosted runner before every merge costs time
without establishing a new trust boundary.

`scripts/land.sh` owns the complete gate, with no changed-path plan. It pushes
the clean branch, runs the Mac and Ubuntu in parallel, and adds Windows
PowerShell 5.1 tests when packaging or PowerShell paths change. Each result
belongs to the exact head. Ubuntu runs detached with a log/end marker and an
exclusive checkout/build lock. Warm target directories are kept; no sccache.
Unreachable machines fail unless explicitly skipped; the PR and status record
those exceptions. [Testing](../testing.md#where-checks-run) documents commands,
machines, logs and invocation.

After passing, preserve existing PR text, append one gate line, set the commit
status **Local gate** to success on that SHA and squash-merge immediately.
The release app's PR uses this same gate. Delete the path classifier, local
check-plan runner, PR placeholders and Release Please CI-dispatch refusal.
Keep title feedback for outside PRs: it is read-only and uses no credentials.
Nix and syntax-tool builds move to main pushes. Release workflows retain full
build, signing, provenance, install/start and publication checks.

Main CI always confirms the full three-system matrix. Daily/manual runs add
heavy qualification. **CI result** still protects nightly publication through
`scripts/ci-main.mjs`; it no longer authorizes merges. A red main preserves the
preceding nightly. This updates the merge/check portions of decisions 0027,
0034 and 0037. Number 0048 was already assigned to Nix.

With warm dependencies and targets, budget about 4–8 minutes from push to merge:
`max(Mac full gate, Ubuntu Rust + Vitest, optional Windows tests) + API calls`.
A cold Rust target can take 15–30 minutes. These are estimates, pending one
real run; hosted confirmation and release builds are outside merge latency.

Applied on 2026-11-13: the required check on main is Local gate; the previous ruleset is kept privately.
