# 0049 Hosted CI gates merges

Decided by the maintainer on 2026-11-14 and last changed on 2026-11-18.

Hosted **CI result** is the required check on main, with no merge queue.
`scripts/land.sh` runs fast local checks, pushes the branch and requests squash
auto-merge; a pull request merges once **CI result** passes, without being up to
date with main, and main's push CI checks the combined commit. Local machines
are for speed and native checks, never the gate.

- **What a pull request runs.** `scripts/ci-plan.mjs` maps each changed path to
  a crate (and every crate that depends on it, from `cargo metadata`), the
  interface or prose; any other path, anything generated or shared, and anything
  it cannot read selects the full suite. **CI result** passes only if every
  selected job passed and every other planned job was skipped. Release checks
  run on a pull request only when a workspace crate changes. A newer push
  cancels the pull request's superseded run.
- **Every system, every time.** The Rust tests run on macOS, Windows and Linux
  whenever they are selected; macOS uses GitHub's hosted runners, and the
  repository variable `RUNNERS` is the switch to our own Mac (Dependabot stays
  hosted).
- **Main and schedules run everything**, as do manual runs, the `ci:full` label
  and the repository variable `CI_SELECTION=full`. Daily and manual runs add
  the heavy qualification jobs.
- **A red main is repaired first.** While main's latest push CI failed,
  `land.sh` lands only `fix` or `revert` titles. One nightly a day is built from
  main's head once its push CI passed; pushes to main publish nothing, and a red
  main keeps the previous nightly.
- **Guarded paths need an independent review.** `land.sh` will not request the
  merge of a change to workflows, scripts, the updater or app permissions
  without a recorded review of its exact head.
