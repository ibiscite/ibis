# Development

How to work on Ibis day to day: one command per job, the machines it runs on,
the landing pitfalls that have cost a round, and how to work with the
maintainer. Rules are in [AGENTS.md](../AGENTS.md); what each check proves is
in [testing](testing.md).

## Set up once

Use `rust-toolchain.toml`, Node 26 and the pnpm version in
`apps/desktop/package.json`, then run
`pnpm -C apps/desktop install --frozen-lockfile`. System packages per platform
are in [CONTRIBUTING.md](../CONTRIBUTING.md#setup). In a Linux cloud session,
`bash scripts/cloud-setup.sh` installs what the desktop crate needs.

## One command per job

| Job | Command |
| --- | --- |
| Start a piece of work (worktree, branch, draft PR) | `bash scripts/land.sh start NAME [ISSUE]` |
| Run the command line | `cargo run -p ibis -- --help` |
| Run the desktop app | `pnpm -C apps/desktop tauri dev` |
| Regenerate bindings, schemas and CLI options | `node scripts/gen.mjs` |
| Check one crate | `cargo check -p CRATE` |
| Test one engine behaviour | `cargo test -p ibis-engine --locked FILTER` |
| Test the app's Rust side (plain `cargo test` skips it) | `cargo test -p ibis-desktop --locked FILTER` |
| Test one interface file | `pnpm -C apps/desktop exec vitest run FILE --maxWorkers=1` |
| Check a pull request title | `node scripts/pr-lint.mjs --title "fix: explain the change"` |
| Build the website | `node website/build.mjs` |
| Rebuild logos and icons from `brand/` | `python3 brand/build.py` |
| Hands-on check of the newest Nightly on a Mac | `bash scripts/hands-on-macos.sh` |
| Land: fast local checks, push, squash auto-merge | `bash scripts/land.sh` |
| Remove the session after its PR merges | `bash scripts/land.sh end NAME` |

Run one focused check for what changed and leave the rest to hosted CI.
Four small skills in [`.agents/skills/`](../.agents/skills/) (linked from
`.claude/skills/` for Claude Code) find problems on demand: `verify`, `review`,
`audit` and `diagnose`. Keep them few; a skill that a check or a structure can
replace is deleted. Git for Windows needs `core.symlinks=true` to see the
Claude Code link.
Specialised checks (links, generated files, the contract report, snapshots)
are in [testing](testing.md#local-commands).

## Where work runs

- **Cloud sessions** (Linux containers) build and test the engine, CLI and
  desktop crate. Disk is a fixed allowance: keep one `target` per worktree,
  build with `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
  CARGO_PROFILE_TEST_DEBUG=0`, and remove worktrees once pushed. GitHub is
  reached through `gh api`; Actions artifact downloads and downloaded binaries
  (such as Quarto) need a maintainer machine. To lint Windows code from Linux:
  `rustup target add x86_64-pc-windows-gnu`, install `gcc-mingw-w64-x86-64`,
  then `cargo clippy -p ibis-desktop --target x86_64-pc-windows-gnu --all-targets --locked`.
- **The maintainer's machines** (a Mac, a Windows laptop and an Ubuntu box) are
  for platform and real-app checks, in parallel, whenever they are reachable.
  Never wait on them: hosted CI decides merges. A dropped remote session is the
  connection, not a sleeping machine; start a new one or move the job. On the
  Mac, run Python through Miniconda's base environment, never the system
  Python.
- **The Mac's self-hosted runner** runs only the macOS CI job, with its own
  toolchain ([testing](testing.md#where-checks-run)). Nothing CI runs there may
  touch the maintainer's home folder or toolchains; check a new action for that
  before routing a job to it.

## Landing pitfalls

- Never squash with `git reset --soft origin/main` after `origin/main` moved:
  the commit silently reverts what landed meanwhile. Rebuild on the new base
  with `git diff OLD_BASE HEAD | git apply --3way`, then check that only your
  files changed.
- Don't force-push a shared branch. To bring a branch up to date, merge
  `origin/main` into it; the squash merge hides the merge commit.
- A pull request with a merge conflict gets no CI run. Merge `origin/main` and
  push again.
- Unresolved review threads block the merge. Resolve each once it is addressed.
- History was rewritten on 2026-10-25. Never push a branch from an older
  clone; clone again.

## Working with the maintainer

Ibis has one maintainer, who directs the work through agents and judges
results by using the app.

- **Decide, then say what you did.** Ask only about the items AGENTS.md
  reserves. Ask once, as a short A/B choice with a recommendation. For look and
  feel, show mock-ups or the running app, not a plan. At a real fork in
  direction, ask early rather than build the wrong thing.
- **Second opinions on big direction.** Before recommending a large change in
  direction or architecture, get an independent read from other models (for
  example Codex Sol and Astra on the maintainer's Mac, read-only) and say where
  they disagree.
- **Hands-on checks are one command.** It sets up everything, runs what a
  script can, and asks only yes/no questions for what needs eyes, as
  `scripts/hands-on-macos.sh` does. Never hand over a manual checklist.
- **Updates are short and plain.** Lead with the result or the one question.
- **Smallest version that works.** One light review of the final result; fix
  its P1 and P2 findings without adding guards for cases nobody has met. Few,
  larger pull requests beat many small ones.
