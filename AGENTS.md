# Agent and contributor guidelines

These rules apply to coding agents and human contributors alike. Product behaviour is in `README.md`, `ROADMAP.md` and `docs/architecture.md`. **How we work** (issues, labels, milestones, tracking issues, branches, commits, pull requests, review and merge) is the [contribution standard](docs/development.md#contribution-standard); follow it exactly. Testing is in `docs/testing.md`. Lasting decisions are in `docs/adr/`; where documents disagree, accepted ADRs win.

Everything needed to continue the work is in this repository and on GitHub: these rules, the docs above, the issues and their milestones, and one tracking issue per active milestone (label `tracking`). Do not rely on notes outside it or on an assistant's memory. `.local/` holds untracked maintainer notes: never commit, quote or summarize them in public text.

## Commands

```sh
node scripts/check.mjs          # everything CI checks: formatting, lints, tests, dependency policy
cargo test -p ibis-core         # fast core tests
cargo test -p ibis-desktop      # desktop crate tests (plain cargo test skips them)
pnpm -C apps/desktop test       # interface tests
cargo run -p ibis -- --help     # command line tool
cargo insta review              # review changed snapshots (cargo install cargo-insta)
node scripts/pr-lint.mjs --title "…" --branch "…" --body-file pr.md   # check a pull request before opening it
```

## Authority

The maintainer delegates engineering, security, testing, CI and documentation decisions to agents acting for Ibis (ADR 0015). Under that standing authorization agents branch, commit, push, open pull requests and merge their own pull requests when the merge rule below is met. Acting for the maintainer they also manage issues, labels, milestones and pull requests: triage against the agreed release scope, keep the tracking issue's Queue and Decisions, post independent reviews, close completed issues and remove merged branches. Agents may accept ADRs that record behaviour already agreed in the current tracking issue, and decide versions and when to publish a release; only Release Please creates tags. None of these routine updates needs another approval.

Ask the maintainer **only** about: licensing, new network use, repository or organization settings, anything that costs money or creates accounts, what goes into or out of a release, and anything needing the maintainer's own hands. Ask once, in plain words, with a recommendation and a one-word answer such as yes/no; label the issue `status:needs-maintainer` and list it under "Waiting on the maintainer" in the tracking issue. Do not ask again for authorization already given. Product defaults users notice (for example the shortcut) are chosen from evidence and stated in the tracking issue so the maintainer can overrule them. Engineering choices are resolved from evidence and the recorded scope, not asked.

Without this authorization (for example an outside contributor's agent), do not commit, push, tag, release or publish unless your user asks for that exact action.

## Roles

Any agent (Claude, Codex or another tool) or human can do any part of the work under the same rules. Whoever merges is the lead. A reviewer is a session that did not write the change.

## The loop

1. **Pick.** Take the first untaken item in the current tracking issue's Queue and say in its issue that you took it. A red `main` comes first, always: revert the merge that broke it, including when the red job is not required but was green before, then fix it in a new pull request.
2. **Branch** `<type>/<issue>-<short-name>` from `origin/main` in your own worktree; never share a checkout. Start with a failing test or a fixture in `fixtures/`.
3. **Build** in small steps, under about 300 changed lines. Use an explicit working directory or absolute paths, and inspect the diff before testing. Update docs and snapshots in the same change; review snapshot diffs like code.
4. **Check** with the fast commands while editing, then `node scripts/check.mjs` on the exact final commit. Report each step as passed, failed or blocked, never "passed" for something that did not run.
5. **Attack your own change once:** hostile, huge or empty input, other scripts and locales, concurrency, each platform, silent failure. P1 = crash, hang, data loss, security; P2 = wrong or silently changed output. Fix P1, P2 and regressions with a test; file P3s as one follow-up issue.
6. **Open** the pull request with the template. Push unfinished work early as a draft so nothing lives on one machine, and mark it ready once the check passes.
7. **Review and merge.** Code needs an independent review of the exact head commit by another session, preferably another model, with every P1 and P2 fixed (filing them as issues does not count). Documentation-only, or tests and synthetic fixtures only, need only your recorded step 5 (ADR 0017). Squash-merge when all required checks pass on that commit. Never use `--admin` or another bypass. When the merge queue is enabled, the release pull request merges like any other; until then, nobody merges to `main` while a release hold is announced on the tracking issue.
8. **Close the loop.** Check `main` is green and keep the tracking issue body current (Now and Queue). Comment on the tracking issue only for decisions and handoffs.

## Pausing and resuming

All state lives on GitHub, so any agent can stop at any point and any other can continue.

- **Before you stop**, push every branch; unfinished work goes up as a draft pull request whose body says what is left. Update Now in the tracking issue body. Leave nothing that only exists on one machine or in one assistant's memory.
- **To resume**, read this file, the tracking issue and the open pull requests, then run `git fetch origin --prune`. Finish open pull requests first: fix red checks, answer reviews, merge. Trust GitHub's current state over any earlier summary.
- **Start prompt** (any agent): "Read AGENTS.md and follow it: resume from the tracking issue and work its Queue."

## Limits

- Finish or close your pull request before starting another; at most one open pull request per agent unless it is waiting only on review.
- No stacking: every branch starts from `main`, because stacked squash merges have silently reverted changes on `main`. After merging `main` into a branch, check `git diff origin/main` shows only your change.
- No new workflows, harnesses, ADRs or dependencies unless the issue asks for them. Prefer maintained libraries to new code and fewer dependencies to more. Dependencies must pass `cargo deny check`; before adding one, confirm on its registry page and repository that the exact name exists and is maintained (AI tools suggest look-alike names).
- Hard-to-reverse decisions (license, persisted data formats, public CLI or API contracts, network use, security boundaries) need an ADR.
- When an agent repeats a mistake, add a check to `scripts/check.mjs` or `scripts/pr-lint.mjs` (preferred) or one line here; remove lines that no longer matter.

## Code, privacy and security

Follow the [architecture principles](docs/architecture.md#principles), including privacy, escaping and endpoints, and the contract documented with each component. No real library data, document text, paths or credentials in code, tests, logs, issues or pull requests.

## Version control safety

- Run `git status` before editing; never reset, restore, delete or reformat work you did not make.
- Before proposing a main-recovery revert, inspect all checks on current `origin/main` and the failure issue's latest lead reviews; an older intermittent failure is not evidence that the newest merge caused it.
- After creating a worktree, set every later command's working directory explicitly; `git worktree add` does not change it. Run repository-relative diff pathspecs from the repository root and confirm they match files before treating an empty diff as equality.
- Never push to `main` or a `claude/*` branch. One branch per pull request.
- Commits follow the [standard](docs/development.md#commits): Conventional Commits in the imperative mood, `!` or a `BREAKING CHANGE:` footer for breaking changes, signed off (`git commit -s`), and, when an AI tool helped, an `Assisted-by: <tool and model>` trailer or the tool's co-author line (ADR 0011). Never put conversation history, pause notes or private context in commits, pull requests or issues.
